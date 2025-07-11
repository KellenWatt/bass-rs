
use libbass::db::{Music, Keyword, self};

use std::fmt::{Display, self, Formatter};

#[derive(Clone, Copy, PartialEq)]
pub enum SearchOp {
    Eq,
    StrEq,
    Lt, 
    Le, // assume it's a number
    Gt,
    Ge, // assume it's a number
    Contains, // assume it's a string
    Has, // keywords only
}

impl Display for SearchOp {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{}", match self {
            SearchOp::Eq => "==",
            SearchOp::StrEq => "'=",
            SearchOp::Lt => "<",
            SearchOp::Le => "<=",
            SearchOp::Gt => ">",
            SearchOp::Ge => ">=",
            SearchOp::Contains => "in",
            SearchOp::Has => "has",
        })
    }
}

impl std::str::FromStr for SearchOp {
    type Err = &'static str;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "==" => Ok(SearchOp::Eq),      
            "'=" => Ok(SearchOp::StrEq),
            "<"  => Ok(SearchOp::Lt),
            "<=" => Ok(SearchOp::Le),
            ">"  => Ok(SearchOp::Gt),
            ">=" => Ok(SearchOp::Ge),
            "in" => Ok(SearchOp::Contains),
            "has" => Ok(SearchOp::Has),
            _ => Err("Not a valid search operation"),
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum Field {
    Title,
    Source,
    Composer,
    Arranger,
    Notes,
    Runtime,
    Keyword,
}

impl Display for Field {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{}", match self {
            Field::Title => "title",
            Field::Source => "source",
            Field::Composer => "composer",
            Field::Arranger => "arranger",
            Field::Notes => "notes",
            Field::Runtime => "runtime",
            Field::Keyword => "keywords",
        })
    }
}

impl std::str::FromStr for Field {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "title" => Ok(Field::Title),
            "source" => Ok(Field::Source),
            "composer" => Ok(Field::Composer),
            "arranger" => Ok(Field::Arranger),
            "notes" => Ok(Field::Notes),
            "runtime" => Ok(Field::Runtime),
            "keywords" => Ok(Field::Keyword),
            _ => Err("Not a valid field"),
        }
    }
}

pub enum SearchType {
    Str(String),
    Num(u16),
}

impl SearchType {
    pub fn from_time(s: &str) -> Result<SearchType, &'static str> {
        let mut time: u16 = 0;
        if s.len() == 0 {
            return Ok(SearchType::Num(0));
        }
        for (i, t) in s.rsplitn(2, ":").enumerate() {
            let t: u16 = t.parse().map_err(|_| "not an integer in time")?;
            time += t* 60u16.pow(i as u32);
        }
        Ok(SearchType::Num(time))
    }

    pub fn as_num(&self) -> u16 {
        if let SearchType::Num(n) = self {
            return *n;
        }
        panic!("Not a number!")
    }
}

impl Display for SearchType {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{}", match self {
            SearchType::Str(s) => s.clone(),
            SearchType::Num(n) => n.to_string(),
        })
    }
}

// explicitly not supporting id search
pub struct Search {
    field: Field,
    invert: bool,
    op: SearchOp,
    right: SearchType,
}

impl Search {
    pub fn new<S: Display>(field: Field, op: SearchOp, right: S, invert: bool) -> Search {
        Search {
            right: if field == Field::Runtime {
                SearchType::from_time(&right.to_string()).unwrap()
            } else {
                SearchType::Str(right.to_string())
            },
            field,
            invert,
            op,
        }
    }

    #[allow(dead_code)]
    pub fn execute(&self) -> db::Result<Vec<Music>> {
        if self.field == Field::Keyword {
            let keywords: Vec<Keyword> = self.right.to_string().split(" ").map(|s| s.parse().unwrap()).collect();
            return Music::by_keywords(&keywords);
        }

        let mut query = Music::query();
        match (self.field, self.op, self.invert) {
            (Field::Title, SearchOp::StrEq, false)  => query.title_eq(&self.right.to_string()),
            (Field::Title, SearchOp::StrEq, true)  => query.title_ne(&self.right.to_string()),
            (Field::Title, SearchOp::Contains, false)  => query.title_contains(&self.right.to_string()),
            (Field::Title, SearchOp::Contains, true)  => query.title_not_contains(&self.right.to_string()),
            
            (Field::Source, SearchOp::StrEq, false)  => query.source_eq(&self.right.to_string()),
            (Field::Source, SearchOp::StrEq, true)  => query.source_ne(&self.right.to_string()),
            (Field::Source, SearchOp::Contains, false)  => query.source_contains(&self.right.to_string()),
            (Field::Source, SearchOp::Contains, true)  => query.source_not_contains(&self.right.to_string()),
            
            (Field::Composer, SearchOp::StrEq, false)  => query.composer_eq(&self.right.to_string()),
            (Field::Composer, SearchOp::StrEq, true)  => query.composer_ne(&self.right.to_string()),
            (Field::Composer, SearchOp::Contains, false)  => query.composer_contains(&self.right.to_string()),
            (Field::Composer, SearchOp::Contains, true)  => query.composer_not_contains(&self.right.to_string()),
            
            (Field::Arranger, SearchOp::StrEq, false)  => query.arranger_eq(&self.right.to_string()),
            (Field::Arranger, SearchOp::StrEq, true)  => query.arranger_ne(&self.right.to_string()),
            (Field::Arranger, SearchOp::Contains, false)  => query.arranger_contains(&self.right.to_string()),
            (Field::Arranger, SearchOp::Contains, true)  => query.arranger_not_contains(&self.right.to_string()),
            
            (Field::Notes, SearchOp::Contains, false)  => query.notes_contains(&self.right.to_string()),
            (Field::Notes, SearchOp::Contains, true)  => query.notes_not_contains(&self.right.to_string()),
        
            (Field::Runtime, SearchOp::Eq, false) => query.runtime_eq(self.right.as_num()),
            (Field::Runtime, SearchOp::Eq, true) => query.runtime_eq(self.right.as_num()),
            (Field::Runtime, SearchOp::Lt, false) => query.runtime_lt(self.right.as_num()),
            (Field::Runtime, SearchOp::Lt, true) => query.runtime_ge(self.right.as_num()),
            (Field::Runtime, SearchOp::Gt, false) => query.runtime_gt(self.right.as_num()),
            (Field::Runtime, SearchOp::Gt, true) => query.runtime_le(self.right.as_num()),
            (Field::Runtime, SearchOp::Le, false) => query.runtime_le(self.right.as_num()),
            (Field::Runtime, SearchOp::Le, true) => query.runtime_gt(self.right.as_num()),
            (Field::Runtime, SearchOp::Ge, false) => query.runtime_ge(self.right.as_num()),
            (Field::Runtime, SearchOp::Ge, true) => query.runtime_lt(self.right.as_num()),
            (f, o, i) => panic!("Not a valid query combination (field: {}, op: {}, inverted: {})", f, o, i),
        };
        query.run()
    }
}

impl Display for Search {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{} {} {} {}", 
            self.field,
            if self.invert {"not"} else {""},
            self.op,
            self.right)
    }
}

impl std::str::FromStr for Search {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let items: Vec<_> = s.splitn(4, ' ').collect();
        let field = items[0].parse()?;
        let invert = items[1] == "not";
        let op = items[2].parse()?;
        let right = items.get(3).ok_or("Not enough fields found")?.to_string();
        Ok(Search::new(
            field,
            op,
            right,
            invert,
        ))
    }
}


