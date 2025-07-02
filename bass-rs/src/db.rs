use std::sync::{OnceLock, Mutex};
use std::convert::Infallible;
use std::path::Path;

use rusqlite::{Connection, named_params, OptionalExtension, Row, params_from_iter};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("database connection has already be initialized")]
    AlreadyInit,
    #[error("database connection has not been established")]
    NotConnected,

    #[error(transparent)]
    DBError(#[from] rusqlite::Error)
}
pub type Result<T> = std::result::Result<T, Error>;

static DB: OnceLock<Mutex<Connection>> = OnceLock::new();

#[allow(dead_code)]
pub fn init<P: AsRef<Path>>(path: Option<P>) -> Result<()> {
    let conn = if let Some(path) = path {
        Connection::open(path)
    } else {
        Connection::open_in_memory()
    }?;
    let init_script = include_str!("bass-init.sql");
    conn.execute_batch(init_script)?;
    DB.set(Mutex::new(conn)).map_err(|_| Error::AlreadyInit)
}

fn execute<P: rusqlite::Params>(stmt: &str, params: P) -> Result<usize> {
    DB.get().ok_or(Error::NotConnected)?.lock().unwrap()
        .execute(stmt, params)
        .map_err(|e| e.into())
}

fn query_row<T, P, F>(query: &str, params: P, f: F) -> Result<Option<T>> 
where P: rusqlite::Params,
      F: FnOnce(&Row<'_>) -> std::result::Result<T, rusqlite::Error> {
    DB.get().ok_or(Error::NotConnected)?.lock().unwrap()
        .query_row(query, params, f) 
        .optional()
        .map_err(|e| e.into())
}

fn query<T, P, F>(query: &str, params: P, f: F) -> Result<Vec<T>> 
where P: rusqlite::Params,
      F: Fn(&Row<'_>) -> std::result::Result<T, rusqlite::Error> {
    let db = DB.get().ok_or(Error::NotConnected)?.lock().unwrap();
    let mut statement = db.prepare_cached(query)?;
    let mut rows = statement
        .query(params)
        .map_err(|e| Error::from(e))?;

    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(f(row)?);
    }

    Ok(out)
}

fn exists<P: rusqlite::Params>(query: &str, params: P) -> Result<bool> {
    let db = DB.get().ok_or(Error::NotConnected)?.lock().unwrap();
    let mut statement = db.prepare_cached(query)?;
    statement.exists(params).map_err(|e| e.into())
}

#[allow(dead_code)]
enum Comparison {
    Has,
    NotHas,
    Less(i32),
    LessEqual(i32),
    Greater(i32),
    GreaterEqual(i32),
    Equal(i32),
    NotEqual(i32),
    FloatLess(f64),
    FloatLessEqual(f64),
    FloatGreater(f64),
    FloatGreaterEqual(f64),
    FloatEqual(f64),
    FloatNotEqual(f64),
    StrEqual(String),
    StrNotEqual(String),
    Contains(String),
    NotContains(String),
}

impl Comparison {
    fn param(&self) -> Option<String> {
        use Comparison::*;
        match self {
            Has | NotHas => None,
            Less(n) | LessEqual(n) | Greater(n) | GreaterEqual(n) | Equal(n) | NotEqual(n) => Some(n.to_string()),
            FloatLess(f) | FloatLessEqual(f) | 
                FloatGreater(f) | FloatGreaterEqual(f) | 
                FloatEqual(f) | FloatNotEqual(f) => Some(f.to_string()),
            StrEqual(ref s) | StrNotEqual(ref s) | Contains(ref s) | NotContains(ref s) => Some(s.clone())
        }
    }
}

pub struct MusicQuery {
    // This can very likely be a &'static str
    conditions: Vec<(String, Comparison)>,
}


#[allow(dead_code)]
impl MusicQuery {
    fn new() -> MusicQuery {
        MusicQuery {
            conditions: Vec::new()
        }
    }

    fn make_query(&self) -> Result<String> {
        use Comparison::*;
        let mut query: String = "SELECT * FROM music".into();
        if self.conditions.len() > 0 {
            query += " WHERE";
        }
        let mut count = 0;
        for cond in self.conditions.iter() {
            count += match cond.1 {
                Has | NotHas => 0,
                _ => 1
            };
            query += &match cond.1 {
                Has => format!(" {} IS NOT NULL", cond.0),
                NotHas => format!(" {} IS NULL", cond.0),
                Less(_) => format!(" {} < ?{}", cond.0, count),
                LessEqual(_) => format!(" {} <= ?{}", cond.0, count),
                Greater(_) => format!(" {} > ?{}", cond.0, count),
                GreaterEqual(_) => format!(" {} >= ?{}", cond.0, count),
                Equal(_) => format!(" {} == ?{}", cond.0, count),
                NotEqual(_) => format!(" {} != ?{}", cond.0, count),
                FloatLess(_) => format!(" {} < ?{}", cond.0, count),
                FloatLessEqual(_) => format!(" {} <= ?{}", cond.0, count),
                FloatGreater(_) => format!(" {} > ?{}", cond.0, count),
                FloatGreaterEqual(_) => format!(" {} >= ?{}", cond.0, count),
                FloatEqual(_) => format!(" {} == ?{}", cond.0, count),
                FloatNotEqual(_) => format!(" {} != ?{}", cond.0, count),
                StrEqual(_) => format!(" {} == ?{}", cond.0, count),
                StrNotEqual(_) => format!(" {} != ?{}", cond.0, count),
                Contains(_) => format!(" instr({}, ?{})", cond.0, count),
                NotContains(_) => format!(" NOT instr({}, ?{})", cond.0, count),
            };
            if self.conditions.len() > 1 {
                query += " AND";
            }
        }
        query += ";";
        Ok(query)
    }


    pub fn run(&self) -> Result<Vec<Music>> {
        let quer = self.make_query()?;
        query(&quer, params_from_iter(self.conditions.iter().filter_map(|c| {
            c.1.param()
        })), |row| Ok(Music::from_row(row)))
    }

    pub fn run_one(&self) -> Result<Option<Music>> {
        let quer = self.make_query()?;
        query_row(&quer, params_from_iter(self.conditions.iter().filter_map(|c| {
            c.1.param()
        })), |row| Ok(Music::from_row(row)))
    }

    pub fn id_eq(&mut self, id: i32) -> &mut Self {
        self.conditions.push(("id".into(), Comparison::Equal(id)));
        self
    }
    
    pub fn id_ne(&mut self, id: i32) -> &mut Self {
        self.conditions.push(("id".into(), Comparison::NotEqual(id)));
        self
    }

    pub fn title_eq(&mut self, title: &str) -> &mut Self {
        self.conditions.push(("title".into(), Comparison::StrEqual(title.into())));
        self
    }
    
    pub fn title_ne(&mut self, title: &str) -> &mut Self {
        self.conditions.push(("title".into(), Comparison::StrNotEqual(title.into())));
        self
    }
    
    pub fn title_contains(&mut self, title: &str) -> &mut Self {
        self.conditions.push(("title".into(), Comparison::Contains(title.into())));
        self
    }
    
    pub fn title_not_contains(&mut self, title: &str) -> &mut Self {
        self.conditions.push(("title".into(), Comparison::NotContains(title.into())));
        self
    }
    
    pub fn source_eq(&mut self, source: &str) -> &mut Self {
        self.conditions.push(("source".into(), Comparison::StrEqual(source.into())));
        self
    }
    
    pub fn source_ne(&mut self, source: &str) -> &mut Self {
        self.conditions.push(("source".into(), Comparison::StrNotEqual(source.into())));
        self
    }
    
    pub fn source_contains(&mut self, source: &str) -> &mut Self {
        self.conditions.push(("source".into(), Comparison::Contains(source.into())));
        self
    }
    
    pub fn source_not_contains(&mut self, source: &str) -> &mut Self {
        self.conditions.push(("source".into(), Comparison::NotContains(source.into())));
        self
    }
    
    pub fn has_composer(&mut self) -> &mut Self {
        self.conditions.push(("composer".into(), Comparison::Has));
        self
    }
    
    pub fn null_composer(&mut self) -> &mut Self {
        self.conditions.push(("composer".into(), Comparison::NotHas));
        self
    }
    
    pub fn composer_eq(&mut self, composer: &str) -> &mut Self {
        self.conditions.push(("composer".into(), Comparison::StrEqual(composer.into())));
        self
    }
    
    pub fn composer_ne(&mut self, composer: &str) -> &mut Self {
        self.conditions.push(("composer".into(), Comparison::StrNotEqual(composer.into())));
        self
    }
    
    pub fn composer_contains(&mut self, composer: &str) -> &mut Self {
        self.conditions.push(("composer".into(), Comparison::Contains(composer.into())));
        self
    }
    
    pub fn composer_not_contains(&mut self, composer: &str) -> &mut Self {
        self.conditions.push(("composer".into(), Comparison::NotContains(composer.into())));
        self
    }

    pub fn has_arranger(&mut self) -> &mut Self {
        self.conditions.push(("arranger".into(), Comparison::Has));
        self
    }
    
    pub fn null_arranger(&mut self) -> &mut Self {
        self.conditions.push(("arranger".into(), Comparison::NotHas));
        self
    }
    
    pub fn arranger_eq(&mut self, arranger: &str) -> &mut Self {
        self.conditions.push(("arranger".into(), Comparison::StrEqual(arranger.into())));
        self
    }
    
    pub fn arranger_ne(&mut self, arranger: &str) -> &mut Self {
        self.conditions.push(("arranger".into(), Comparison::StrNotEqual(arranger.into())));
        self
    }
    
    pub fn arranger_contains(&mut self, arranger: &str) -> &mut Self {
        self.conditions.push(("arranger".into(), Comparison::Contains(arranger.into())));
        self
    }
    
    pub fn arranger_not_contains(&mut self, arranger: &str) -> &mut Self {
        self.conditions.push(("arranger".into(), Comparison::NotContains(arranger.into())));
        self
    }
   
    pub fn has_notes(&mut self) -> &mut Self {
        self.conditions.push(("notes".into(), Comparison::Has));
        self
    }

    pub fn null_notes(&mut self) -> &mut Self {
        self.conditions.push(("notes".into(), Comparison::NotHas));
        self
    }

    pub fn notes_eq(&mut self, notes: &str) -> &mut Self {
        self.conditions.push(("notes".into(), Comparison::StrEqual(notes.into())));
        self
    }
    
    pub fn notes_ne(&mut self, notes: &str) -> &mut Self {
        self.conditions.push(("notes".into(), Comparison::StrNotEqual(notes.into())));
        self
    }
    
    pub fn notes_contains(&mut self, notes: &str) -> &mut Self {
        self.conditions.push(("notes".into(), Comparison::Contains(notes.into())));
        self
    }
    
    pub fn notes_not_contains(&mut self, notes: &str) -> &mut Self {
        self.conditions.push(("notes".into(), Comparison::NotContains(notes.into())));
        self
    }

    pub fn has_runtime(&mut self) -> &mut Self {
        self.conditions.push(("runtime".into(), Comparison::Has));
        self
    }
    
    pub fn null_runtime(&mut self) -> &mut Self {
        self.conditions.push(("runtime".into(), Comparison::NotHas));
        self
    }

    pub fn runtime_eq(&mut self, runtime: u16) -> &mut Self {
        self.conditions.push(("runtime".into(), Comparison::Equal(runtime.into())));
        self
    }
    pub fn runtime_ne(&mut self, runtime: u16) -> &mut Self {
        self.conditions.push(("runtime".into(), Comparison::NotEqual(runtime.into())));
        self
    }
    pub fn runtime_lt(&mut self, runtime: u16) -> &mut Self {
        self.conditions.push(("runtime".into(), Comparison::Less(runtime.into())));
        self
    }
    pub fn runtime_le(&mut self, runtime: u16) -> &mut Self {
        self.conditions.push(("runtime".into(), Comparison::LessEqual(runtime.into())));
        self
    }
    pub fn runtime_gt(&mut self, runtime: u16) -> &mut Self {
        self.conditions.push(("runtime".into(), Comparison::Greater(runtime.into())));
        self
    }
    pub fn runtime_ge(&mut self, runtime: u16) -> &mut Self {
        self.conditions.push(("runtime".into(), Comparison::GreaterEqual(runtime.into())));
        self
    }
    
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct Music {
    id: Option<i32>,
    pub title: String,
    pub source: String,
    pub composer: Option<String>,
    pub arranger: Option<String>,
    pub notes: Option<String>,
    pub runtime: Option<u16>,
}

#[allow(dead_code)]
impl Music {
    fn from_row(row: &Row<'_>) -> Music {
        Music {
            id: row.get_unwrap(0),
            title: row.get_unwrap(1),
            source: row.get_unwrap(2),
            composer: row.get_unwrap(3),
            arranger: row.get_unwrap(4),
            notes: row.get_unwrap(5),
            runtime: row.get_unwrap(6),
        }
    }

    pub fn new() -> Music {
        Music {
            id: None,
            title: "".into(),
            source: "".into(),
            composer: None,
            arranger: None,
            notes: None,
            runtime: None,
        }
    }

    pub fn new_with_id(id: i32) -> Music {
        let mut m = Music::new();
        m.id = Some(id);
        m
    }

    pub fn id(&self) -> Option<i32> {
        self.id.clone()
    }

    pub fn query() -> MusicQuery {
        MusicQuery::new()
    }

    pub fn list_all() -> Result<Vec<Music>> {
        query("SELECT * FROM music;", named_params!{}, |row| {
            Ok(Music::from_row(row))
        })
    }

    pub fn by_id(id: i32) -> Result<Option<Music>> {
        // query_row("SELECT * FROM music WHERE id = :id;", named_params!{":id": id}, |row| {
        //     Ok(Music::from_row(row))
        // })
        Self::query().id_eq(id).run_one()
    }

    pub fn by_title(title: &str) -> Result<Vec<Music>> {
        // query("SELECT * FROM music WHERE title = :title;", named_params!{":title": title}, |row| {
        //     Ok(Music::from_row(row))
        // })
        Self::query().title_eq(title).run()
    }

    pub fn by_source(source: &str) -> Result<Vec<Music>> {
        // query("SELECT * FROM music WHERE source = :source;", named_params!{":source": source}, |row| {
        //     Ok(Music::from_row(row))
        // })
        Self::query().source_eq(source).run()
    }
    pub fn by_composer(composer: &str) -> Result<Vec<Music>> {
        // query("SELECT * FROM music WHERE composer = :composer;", named_params!{":composer": composer}, |row| {
        //     Ok(Music::from_row(row))
        // })
        Self::query().composer_eq(composer).run()
    }
    pub fn by_arranger(arranger: &str) -> Result<Vec<Music>> {
        // query("SELECT * FROM music WHERE arranger = :arranger;", named_params!{":arranger": arranger}, |row| {
        //     Ok(Music::from_row(row))
        // })
        Self::query().arranger_eq(arranger).run()
    }
    
    pub fn notes_contains(keyword: &str) -> Result<Vec<Music>> {
        // query("SELECT * FROM music WHERE instr(notes, :keyword);", named_params!{":keyword": keyword}, |row| {
        //     Ok(Music::from_row(row))
        // })
        Self::query().notes_contains(keyword).run()
    }

    pub fn by_keywords(keywords: &[Keyword]) -> Result<Vec<Music>> {
        if keywords.len() == 0 {
            return Ok(Vec::new());
        }
        let key_ids = Keyword::fetch_ids(keywords)?.into_iter().map(|id| id.to_string()).collect::<Vec<_>>().join(",");
        query(&format!("SELECT m.id, m.title, m.composer, m.arranger, m.source, m.notes, m.runtime FROM music m
               INNER JOIN music_keywords mk ON m.id == mk.mid
               WHERE mk.kid IN ({});", key_ids), (), |row| Ok(Music::from_row(row)))
    }
    
    pub fn list_titles() -> Result<Vec<String>> {
        query("SELECT DISTINCT title FROM music;", (), |row| {
            Ok(row.get(0)?)
        })
    }
    
    pub fn list_composers() -> Result<Vec<String>> {
        query("SELECT DISTINCT composer FROM music;", (), |row| {
            Ok(row.get(0)?)
        })
    }
    pub fn list_arrangers() -> Result<Vec<String>> {
        query("SELECT DISTINCT arranger FROM music;", (), |row| {
            Ok(row.get(0)?)
        })
    }

    pub fn list_sources() -> Result<Vec<String>> {
        query("SELECT DISTINCT source FROM music;", (), |row| {
            Ok(row.get(0)?)
        })
    }

    pub fn keywords(&self) -> Result<Option<Vec<Keyword>>> {
        if self.id.is_none() {
            return Ok(None);
        }
        // get associated keywords from DB
        Ok(Some(query("
            SELECT k.id, k.category, k.keyword FROM keywords k 
            INNER JOIN music_keywords mk ON k.id = mk.kid
            WHERE mk.mid = :id;", 
            named_params!{":id": self.id}, 
            |row| {
                Ok(Keyword::from_row(row))
            })?))
    }

    pub fn insert(&mut self) -> Result<()> {
        if !self.is_db_entry() {
            execute("INSERT INTO music (title, composer, arranger, source, notes, runtime) VALUES (
                :title,
                :composer,
                :arranger,
                :source,
                :notes,
                :runtime
            );", named_params!{
                ":title": self.title,
                ":composer": self.composer,
                ":arranger": self.arranger,
                ":source": self.source,
                ":notes": self.notes,
                ":runtime": self.runtime,
            }).map(|_| ())?;
            let id: i32 = query_row("SELECT max(id) from music LIMIT 1;", (), |row| {
                Ok(row.get_unwrap(0))
            })?.unwrap(); // Yes, this heavy duplication of Options is intentional
            self.id = Some(id);
            Ok(())
        } else {
            execute("UPDATE music SET
                title = :title,
                composer = :composer,
                arranger = :arranger,
                source = :source,
                notes = :notes,
                runtime = :runtime
            WHERE id = :id;", named_params!{
                ":id": self.id,
                ":title": self.title,
                ":composer": self.composer,
                ":arranger": self.arranger,
                ":source": self.source,
                ":notes": self.notes,
                ":runtime": self.runtime,
            }).map(|_| ())
        }
    }

    pub fn insert_with_keywords(&mut self, keys: &mut [Keyword]) -> Result<()> {
        self.insert()?;
        // TODO insert keys if they don't exist, then form pairings
        for key in keys.iter_mut() {
            if !key.exists()? {
                key.insert_update()?;
            }
            if !key.is_db_entry() {
                key.update_self()?;
            }
            // We are assuming at this point that key has a valid id
            execute("INSERT INTO music_keywords VALUES (:mid, :kid) ON CONFLICT DO NOTHING;", named_params!{
                ":mid": self.id,
                ":kid": key.id,
            })?;
        }
        Ok(())
    }

    pub fn update_keywords(&mut self, keys: &mut [Keyword]) -> Result<()> {
        let keywords = self.keywords()?.unwrap();
        let to_insert = keys.iter().filter(|k| !keywords.contains(k));
        let to_remove = keywords.iter().filter(|k| !keys.contains(k));
        for key in to_insert {
            let mut key = key.clone();
            self.add_keyword(&mut key)?;
        }
        for key in to_remove {
            let mut key = key.clone();
            self.remove_keyword(&mut key)?;
        }
        Ok(())
    }

    pub fn remove_keyword(&mut self, key: &mut Keyword) -> Result<()> {
        let Some(keywords) = self.keywords()? else {
            // ignore trying to delete non-db
            return Ok(());
        };

        if keywords.contains(key) {
            if !key.is_db_entry() {
                key.update_self()?;
            }
            execute("DELETE FROM music_keywords WHERE mid = :mid AND kid = :kid;", named_params! {
                ":mid": self.id,
                ":kid": key.id,
            })?;
        }
        Ok(())
    }

    pub fn add_keyword(&mut self, key: &mut Keyword) -> Result<()> {
        // FIXME should probably add something more to handle being non-db, like an error
        if !self.is_db_entry() {
            return Ok(());
        }
        if !key.exists()? {
            key.insert_update()?;
        }
        if !key.is_db_entry() {
            key.update_self()?;
        }
        // We are assuming at this point that key has a valid id
        execute("INSERT INTO music_keywords VALUES (:mid, :kid) ON CONFLICT DO NOTHING;", named_params!{
            ":mid": self.id,
            ":kid": key.id,
        })?;
        Ok(())
    }

    pub fn delete(self) -> Result<()> {
        execute("DELETE FROM music WHERE id = :id;", named_params! {
            ":id": self.id,
        })?;
        Ok(())
    }

    pub fn is_db_entry(&self) -> bool {
        self.id.is_some()
    }
}

#[derive(Clone, Debug)]
pub struct Keyword {
    id: Option<i32>,
    pub category: Option<String>,
    pub keyword: String,
}

#[allow(dead_code)]
impl Keyword {
    fn from_row(row: &Row) -> Keyword {
        Keyword {
            id: row.get_unwrap(0),
            category: row.get_unwrap(1),
            keyword: row.get_unwrap(2),
        }
    }

    pub fn list_all() -> Result<Vec<Keyword>> {
        query("SELECT * FROM keywords;", (), |row| {
            Ok(Keyword::from_row(row))
        })
    }

    pub fn list_categories() -> Result<Vec<String>> {
        query("SELECT DISTINCT category FROM keywords;", (), |row| {
            row.get(0)
        })
    }

    pub fn new(keyword: &str) -> Keyword {
        keyword.parse().unwrap()
    }

    pub fn is_db_entry(&self) -> bool {
        self.id.is_some()
    }

    pub fn exists(&self) -> Result<bool> {
        let (query, params) = if self.is_db_entry() {
            ("SELECT * FROM keywords WHERE id = :id;", named_params!{":id": self.id})
        } else {
            ("SELECT * FROM keywords WHERE category IS :category AND keyword == :keyword;", 
             named_params!{":category": self.category, ":keyword": self.keyword})
        };
        exists(query, params)
    }

    pub fn insert(&mut self) -> Result<()> {
        if !self.is_db_entry() {
            execute("INSERT INTO keywords (category, keyword) VALUES (:category, :keyword);", named_params!{
                ":category": self.category,
                ":keyword": self.keyword,
            }).map(|_| ())?;
            let id = query_row("SELECT max(id) FROM keywords LIMIT 1;", (), |row| {
                Ok(row.get_unwrap(0))
            })?.unwrap();
            self.id = Some(id);
            Ok(())
        } else {
            execute("UPDATE keywords SET category = :category, keyword = :keyword WHERE id == :id;", named_params!{
                ":id": self.id,
                ":category": self.category,
                ":keyword": self.keyword,
            }).map(|_| ())
        }
    }

    pub fn insert_update(&mut self) -> Result<()> {
        if !self.exists()? {
            self.insert()?
        }
        self.update_self()?;
        Ok(())
    }
    
    fn update_self(&mut self) -> Result<()> {
        let id = query_row("SELECT id FROM keywords WHERE category IS :category AND keyword == :keyword;",
            named_params!{
                ":category": self.category,
                ":keyword": self.keyword,
            }, 
            |row| Ok(row.get_unwrap(0)))?;
        self.id = id;
        Ok(())
    }

    fn fetch_ids(keys: &[Keyword]) -> Result<Vec<usize>> {
        if keys.len() == 0 {
            return Ok(Vec::new());
        }
        let mut params = Vec::new();
        let conditions = keys.iter().enumerate().map(|(i, k)| {
            params.push(k.category.as_ref());
            params.push(Some(&k.keyword));
            format!("category IS ?{} AND keyword == ?{}", 2*i+1, 2*(i+1))
        }).collect::<Vec<_>>().join(" OR ");
        let quer = format!("SELECT id FROM keywords WHERE {};", conditions);
        query(&quer, params_from_iter(params.into_iter()), |row| {
            Ok(row.get_unwrap(0))
        })
    }
}

impl std::str::FromStr for Keyword {
    type Err = Infallible;
    fn from_str(s: &str) -> std::result::Result<Keyword, Self::Err> {
        let segments = s.splitn(2, ':').collect::<Vec<_>>();
        if segments.len() == 1 {
            Ok(Keyword {
                id: None,
                category: None,
                keyword: segments[0].to_owned(),
            })
        } else {
            Ok(Keyword {
                id: None,
                category: Some(segments[0].to_owned()),
                keyword: segments[1].to_owned(),
            })
        }
    }
}

impl std::fmt::Display for Keyword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.category.is_none() {
            write!(f, "{}", self.keyword)
        }
        else {
            write!(f, "{}:{}", self.category.as_ref().unwrap(), self.keyword)
        }
    }
}

impl PartialEq for Keyword {
    fn eq(&self, other: &Keyword) -> bool {
        self.category == other.category && self.keyword == other.keyword
    }
}

/*
use regex::Regex;

// referencing scientific pitch notation. C0 has u8 value 0
pub struct Pitch(u8);

pub type PitchError = &'static str;

impl std::fmt::Display for Pitch {
    fn fmt(&self, &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let letter = match self.0 % 12 {
            0  => "C",
            1  => "C♯/D♭",
            2  => "D",
            3  => "D♯/E♭",
            4  => "E",
            5  => "F",
            6  => "F♯/G♭",
            7  => "G",
            8  => "G♯/A♭",
            9  => "A",
            10 => "A♯/B♭",
            11 => "B",
        };

        let octave = self.0 / 12;
        write!("{}{}", letter, octave)
    }
}

static NOTE_PATTERN: std::sync::OnceLock<Regex> = OnceLock::new();

// allow C sharp/C flat/C♯/C♭/C#/Cb
impl std::str::FromStr for Pitch {
    type Err = PitchError;
    fn from_str(s: &str) -> Result<Pitch, Self::Err> {
        let pattern = NOTE_PATTERN.get_or_init(|| {
            Regex::new(r"(?<letter>[a-gA-G])(?<modifier>sharp|flat|#|b|♯|♭)?(?<octave>10|[0-9])").unwrap()
        });
        let caps = pattern.captures(s).map_err(|| "Invalid note")?;
        let letter = caps.name("letter").unwrap();
        let modifier = caps.name("modifier");
        let octave: u8 = caps.name("octave").unwrap().parse().unwrap();
        let code = match letter.to_ascii_uppercase() {
            "C" => 0,
            "D" => 2,
            "E" => 4,
            "F" => 5,
            "G" => 7,
            "A" => 9,
            "B" => 11,
            _ => unreachable!()
        };
        let code = match modifier {
            None => code,
            Some(m) => {
                match m {
                    "sharp" | "#" | "♯" => {
                        code + 1
                    }
                    "flat" | "b" | "♭" => {
                        code - 1
                    }
                    _ => unreachable!()
                }
            }
        };

        let code = octave * 12 + code;
        Ok(Pitch(code))
    }
}

impl From<u8> for Pitch {
    fn from(value: u8) -> Pitch {
        Pitch(value)
    }
}

impl Into<u8> for Pitch {
    fn into(self) -> u8 {
        self.0
    }
}
*/
