#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
slint::include_modules!();
use slint::Model;

use log::*;
use std::io::Write;
use std::path::{Path, PathBuf};

use std::rc::Rc;
use std::sync::{LazyLock, RwLock, Arc};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{JoinHandle, self};
use std::time::{Instant, Duration};

use libbass::db::{self, Music as DBMusic, Keyword};
mod search;
mod config;
use config::Config;


// Slint Type
// export struct Music {
//     id: int,
//     title: string,
//     source: string,
//     composer: string,
//     arranger: string,
//     notes: string,
//     runtime: int,
//     keywords: [string],
// }
// Rust Type
// pub struct Music {
//     id: Option<i64>,
//     pub title: String,
//     pub source: String,
//     pub composer: Option<String>,
//     pub arranger: Option<String>,
//     pub notes: Option<String>,
//     pub runtime: Option<u16>,
// }

#[allow(dead_code)]
struct WordProvider {
    thread: JoinHandle<()>,
    kill: Arc<AtomicBool>,
    cache: Arc<RwLock<Vec<Keyword>>>,
}

impl WordProvider {
    fn new(update_interval: Duration) -> WordProvider {
        let cache = Arc::new(RwLock::new(Vec::new()));
        let kill = Arc::new(AtomicBool::new(false));
        let thread_cache = cache.clone();
        let thread_kill = kill.clone();
        *cache.write().unwrap() = Keyword::list_all().unwrap();
        WordProvider {
            thread: thread::spawn(move || {
                let mut next_time = Instant::now();
                while !thread_kill.load(Ordering::Acquire) {
                    if Instant::now() < next_time {
                        thread::sleep(Duration::from_millis(200));
                        continue;
                    }

                    let mut keys = Keyword::list_all().unwrap();
                    {
                        let mut writer = thread_cache.write().unwrap();
                        writer.clear();
                        writer.append(&mut keys);
                    }
                    next_time = Instant::now() + update_interval;
                }
            }),
            kill,
            cache,
        }
    }

    fn words(&self) -> Vec<Keyword> {
        self.cache.read().unwrap().clone()
    }

    #[allow(dead_code)]
    fn stop(self) {
        // feels like calling this should be necessary, but is apparently not with slint
        self.kill.store(true, Ordering::Release);

        let _ = self.thread.join();
    }
}

static WORD_PROVIDER: LazyLock<WordProvider> = LazyLock::new(|| {
    WordProvider::new(Duration::from_secs(5))
});

static CURRENT_SEARCH: RwLock<Option<UISearch>> = RwLock::new(None);

fn music_from_ui(m: &Music) -> DBMusic {
    let mut dbm = DBMusic::new_with_id(m.id.into());
    dbm.title = m.title.clone().into();
    dbm.source = m.source.clone().into();
    dbm.composer = if m.composer.is_empty() {None} else {Some(m.composer.clone().into())};
    dbm.arranger = if m.arranger.is_empty() {None} else {Some(m.arranger.clone().into())};
    dbm.notes = if m.notes.is_empty() {None} else {Some(m.notes.clone().into())};
    dbm.runtime = if m.runtime == 0 {None} else {
        let time: i32 = m.runtime.into();
        Some(time as u16)
    };
    dbm
}

fn words_by_hint(hint: slint::SharedString) -> slint::ModelRc<slint::SharedString> {
    let hint = hint.to_string();
    let words = WORD_PROVIDER.words();
    let keys: Vec<slint::SharedString> = words.into_iter().filter_map(|k| {
        (k.category.as_ref().map_or(false, |c| c.starts_with(&hint)) || k.keyword.starts_with(&hint))
            .then(|| k.to_string().into())
    }).collect();
    Rc::new(slint::VecModel::from(keys)).into()
}

macro_rules! attempt {
    ($t:expr) => {
        match $t {
            Ok(s) => s,
            Err(e) => {
                error!("{}", e);
                std::process::exit(1);
            }
        }
    };
    ($msg:expr, $t:expr) => {
        match $t {
            Ok(s) => s,
            Err(e) => {
                error!("{} {}", $msg, e);
                std::process::exit(1);
            }
        }
    }
}

macro_rules! assume {
    ($t:expr) => {
        match $t {
            Some(s) => s,
            None => {
                error!("Assumption didn't hold at {}", line!());
                std::process::exit(1);
            }
        }
    };
    ($msg:expr, $t:expr) => {
        match $t {
            Some(s) => s,
            None => {
                error!("{}", $msg);
                std::process::exit(1);
            }
        }
    }
}

fn validate_time(t: slint::SharedString) -> bool {
    let mut has_colon = false;
    t.chars().all(|c| {
        if c == ':' {
            has_colon = true;
        }
        c.is_ascii_digit()
    }) || (has_colon && t.split(":").all(|s| {
        (s.len() == 1 || s.len() == 2) && s.parse::<u16>().unwrap() < 60
    }))
}

#[allow(dead_code)]
fn find_dbs<P: AsRef<Path>>(file: P) -> std::io::Result<Vec<PathBuf>> {
    Ok(std::fs::read_dir(file)?.filter_map(|entry| {
        Some(entry.ok()?.path())
    }).collect())
}

fn main() -> Result<(), slint::PlatformError> {

    let Some(home) = std::env::home_dir() else {
        panic!("Program not being run as as a user");
    };

    let file_root = home
                    .join("Library")
                    .join("Application Support")
                    .join("Bass");
    if !file_root.is_dir() {
        std::fs::create_dir(&file_root).unwrap();
    }
    let config = Config::load(&file_root);
    let config = Arc::new(RwLock::new(config));
    
    simplelog::WriteLogger::init(
        simplelog::LevelFilter::Trace, 
        simplelog::Config::default(), 
        std::fs::File::create(file_root.join("log.txt")).unwrap()).unwrap();

    let database_files = file_root.join("dbs");
    if !database_files.is_dir() {
        attempt!(std::fs::create_dir(&database_files));
    }

    let last_db = attempt!(config.read()).last_db.clone().unwrap_or("collection.sqlite3".into());
    // TODO maybe someday. Allow for multiple different dbs.

    attempt!(db::init(Some(database_files.join(&last_db))));
    attempt!(config.write()).last_db = Some("collection.sqlite3".into());
    
    let main_window = Bass::new()?;
    let add_dialog = AddDialog::new()?;
    let search_dialog = SearchDialog::new()?;

    main_window.global::<KeywordInputLogic>().on_words(words_by_hint);
    add_dialog.global::<KeywordInputLogic>().on_words(words_by_hint);
    search_dialog.global::<KeywordInputLogic>().on_words(words_by_hint);
    search_dialog.global::<BusinessLogic>().on_validate_time(validate_time);

    add_dialog.on_validate_time(validate_time);
    let weak_add = add_dialog.as_weak();
    add_dialog.on_cancel_clicked(move || {
        let dialog = weak_add.unwrap();
        attempt!(dialog.hide());
        dialog.invoke_clear_form();
    });


    let weak_add = add_dialog.as_weak();
    add_dialog.on_update_keywords(move |keyword| {
        let dialog = weak_add.unwrap();
        let keys = dialog.get_keywords();
        let keys = keys.as_any().downcast_ref::<slint::VecModel<slint::SharedString>>().unwrap();
        if keys.iter().any(|k| k == keyword) { return; };

        keys.push(keyword);
    });

    let weak_add = add_dialog.as_weak();
    add_dialog.on_remove_keyword(move |idx, _| {
        let dialog = weak_add.unwrap();
        let keys = dialog.get_keywords();
        let keys = keys.as_any().downcast_ref::<slint::VecModel<slint::SharedString>>().unwrap();
        keys.remove(idx as usize);
    });

    let weak_add = add_dialog.as_weak();
    add_dialog.on_clear_keywords(move || {
        let dialog = weak_add.unwrap();
        dialog.set_keywords(Rc::new(slint::VecModel::default()).into());
    });

    let weak_add = add_dialog.as_weak();
    let weak_main = main_window.as_weak();
    add_dialog.on_submit(move |out, runtime| {
        let dialog = weak_add.unwrap();
        let main_window = weak_main.unwrap();
        let mut music = DBMusic::new();
        music.title = out.title.into();
        music.source = out.source.into();
        if out.composer.len() > 0 {
            music.composer = Some(out.composer.into());
        }
        if out.arranger.len() > 0 {
            music.arranger = Some(out.arranger.into());
        }
        if out.notes.len() > 0 {
            music.notes = Some(out.notes.into());
        }
        if runtime.len() > 0 {
            let runtime = runtime.rsplit(":").enumerate().fold(0, |acc, (i, r)| {
                let r: u16 = r.parse().unwrap();
                acc + r * 60u16.pow(i as u32)
            });
            music.runtime = Some(runtime);
        }
        let mut keywords: Vec<Keyword> = out.keywords.iter().map(|k| k.parse().unwrap()).collect();
        attempt!(music.insert_with_keywords(&mut keywords));
        main_window.invoke_trigger_refresh();
        let _ = dialog.hide();
        dialog.invoke_clear_form();
    });

    let weak_search = search_dialog.as_weak();
    search_dialog.on_cancel_clicked(move || {
        let dialog = weak_search.unwrap();
        attempt!(dialog.hide());
    });

    let weak_main = main_window.as_weak();
    let weak_search = search_dialog.as_weak();
    search_dialog.on_submit(move |s| {
        let main_window = weak_main.unwrap();
        let search_dialog = weak_search.unwrap();
        main_window.invoke_search(s);
        attempt!(search_dialog.hide());
    });
    

    let weak_add = add_dialog.as_weak();
    main_window.on_show_add_dialog(move || {
        let dialog = weak_add.unwrap();
        attempt!(dialog.show());
    });

    main_window.on_update_entry(move |m| {
        let mut music = music_from_ui(&m);
        let mut keywords: Vec<Keyword> = m.keywords.iter().map(|k| k.parse().unwrap()).collect();

        let _ = music.update_keywords(&mut keywords);
        let _ = music.insert();
    });

    main_window.on_remove_keyword(move |m, idx, key| {
        let mut music = music_from_ui(&m);
        let mut key = key.parse().unwrap();
        let _ = attempt!(music.remove_keyword(&mut key));

        let keys = m.keywords.as_any().downcast_ref::<slint::VecModel<slint::SharedString>>().unwrap();
        keys.remove(idx as usize);
    });

    main_window.on_remove_entry(move |m| {
        let music = music_from_ui(&m);
        let _ = attempt!(music.delete());
    });

    main_window.on_add_keyword(move |m, k| {
        let mut music = music_from_ui(&m);
        let mut key = k.parse().unwrap();
        if !assume!(attempt!(music.keywords())).contains(&key) {
            let _ = attempt!(music.add_keyword(&mut key));

            let keys = m.keywords.as_any().downcast_ref::<slint::VecModel<slint::SharedString>>().unwrap();
            keys.push(k);
        }
    });

    let weak_search = search_dialog.as_weak();
    main_window.on_show_search_dialog(move || {
        let dialog = weak_search.unwrap();
        dialog.invoke_clear();
        attempt!(dialog.show());
    });

    let weak_main = main_window.as_weak();
    main_window.on_search(move |s| {
        let main_window = weak_main.unwrap();
        *attempt!(CURRENT_SEARCH.write()) = Some(s);
        main_window.set_results_filtered(true);
        main_window.invoke_trigger_refresh();
    });

    let weak_main = main_window.as_weak();
    main_window.on_clear_search(move || {
        let main_window = weak_main.unwrap();
        *attempt!(CURRENT_SEARCH.write()) = None;
        main_window.set_results_filtered(false);
        main_window.invoke_trigger_refresh();
    });

    let weak_main = main_window.as_weak();
    main_window.on_trigger_refresh(move || {
        let main_window = weak_main.unwrap();

        let musics = match *attempt!(CURRENT_SEARCH.read()) {
            Some(ref s) => {
                let text = s.search_text.to_string();
                let search: search::Search = attempt!(text.parse());
                attempt!(search.execute())
            }
            None => {
                attempt!(DBMusic::list_all())
            }
        };
        
        let music_list = musics.into_iter().map(|m| {
            let keywords = assume!(attempt!(m.keywords()));
            let keywords: Vec<slint::SharedString> = keywords.into_iter().map(|k| k.to_string().into()).collect();
            let keymodel = Rc::new(slint::VecModel::from(keywords));
            Music {
                id: assume!(m.id()).into(),
                title: m.title.into(),
                source: m.source.into(),
                composer: m.composer.unwrap_or("".into()).into(),
                arranger: m.arranger.unwrap_or("".into()).into(),
                notes: m.notes.unwrap_or("".into()).into(),
                runtime: m.runtime.unwrap_or(0).into(),
                keywords: keymodel.into(),
            }
        });
        let model = Rc::new(slint::VecModel::from_iter(music_list));
        main_window.set_music_list(model.into());
    });

    let weak_main = main_window.as_weak();
    let tmp_file_root = file_root.clone();
    main_window.on_add_search(move |name| {
        // We'll assume that this is never called without there being an existing search
        let mut search = assume!((*attempt!(CURRENT_SEARCH.read())).clone());
        search.name = name;
        
        let main_window = weak_main.unwrap();
        let searches = main_window.get_saved_searches();
        let searches = searches.as_any().downcast_ref::<slint::VecModel<UISearch>>().unwrap();
        searches.push(search);

        let tmp_search_file = tmp_file_root.join("tmp_searches.txt");
        let mut f = attempt!(std::fs::File::create(&tmp_search_file));
        for s in searches.iter() {
            attempt!(write!(f, "{}: {}", s.name, s.search_text));
        }
        drop(f);
        let search_file = tmp_file_root.join("searches.txt");
        attempt!(std::fs::rename(tmp_search_file, search_file));
    });

    let weak_main = main_window.as_weak();
    let tmp_file_root = file_root.clone();
    main_window.on_remove_search(move |i| {
        let main_window = weak_main.unwrap();
        let searches = main_window.get_saved_searches();
        let searches = searches.as_any().downcast_ref::<slint::VecModel<UISearch>>().unwrap();

        searches.remove(i as usize);
        
        let tmp_search_file = tmp_file_root.join("tmp_searches.txt");
        let mut f = attempt!(std::fs::File::create(&tmp_search_file));
        for s in searches.iter() {
            attempt!(write!(f, "{}: {}", s.name, s.search_text));
        }
        drop(f);
        let search_file = tmp_file_root.join("searches.txt");
        std::fs::rename(tmp_search_file, search_file).unwrap();
    });

    let weak_main = main_window.as_weak();
    let tmp_file_root = file_root.clone();
    main_window.on_refresh_searches(move || {
        let main_window = weak_main.unwrap();
        let search_file = tmp_file_root.join("searches.txt");
        if !search_file.exists() {
            return
        }
        let searches = attempt!(std::fs::read_to_string(search_file));

        let searches = searches.lines().map(|l| {
            let (name, search) = assume!(l.split_once(": "));
            UISearch {
                name: name.into(),
                search_text: search.into(),
            }
        });
        main_window.set_saved_searches(Rc::new(slint::VecModel::from_iter(searches)).into());
    });

    let weak_main = main_window.as_weak();
    let weak_add = add_dialog.as_weak();
    let weak_search = search_dialog.as_weak();
    let dup_config = config.clone();
    main_window.on_update_default_font_size(move |action| {
        let main_window = weak_main.unwrap();
        let add_dialog = weak_add.unwrap();
        let search_dialog = weak_search.unwrap();
        match action {
            FontSizeAction::Default => {
                let default_size = attempt!(dup_config.read()).ui.default_font_size;
                main_window.set__default_font_size(default_size);
                add_dialog.set__default_font_size(default_size + 2.0);
                search_dialog.set__default_font_size(default_size + 2.0);
            }
            FontSizeAction::Increase => {
                main_window.set__default_font_size(main_window.get__default_font_size() + 2.0);
                add_dialog.set__default_font_size(add_dialog.get__default_font_size() + 2.0);
                search_dialog.set__default_font_size(search_dialog.get__default_font_size() + 2.0);
                attempt!(dup_config.write()).ui.default_font_size = main_window.get__default_font_size();
            }
            FontSizeAction::Decrease => {
                main_window.set__default_font_size(main_window.get__default_font_size() - 2.0);
                add_dialog.set__default_font_size(add_dialog.get__default_font_size() - 2.0);
                search_dialog.set__default_font_size(search_dialog.get__default_font_size() - 2.0);
                attempt!(dup_config.write()).ui.default_font_size = main_window.get__default_font_size();
            }
        }
    });
   
    let weak_main = main_window.as_weak();
    let tmp_file_root = database_files.clone();
    main_window.on_export_db(move || {
        let main_window = weak_main.unwrap();
        let file_name = rfd::FileDialog::new()
            .set_directory("~")
            .set_file_name("bass.sqlite3")
            .set_parent(&main_window.window().window_handle())
            .set_can_create_directories(true)
            .save_file();
        if let Some(file_name) = file_name {
            attempt!(std::fs::copy(tmp_file_root.join("collection.sqlite3"), file_name));
        }
    });

    main_window.invoke_trigger_refresh();
    main_window.invoke_refresh_searches();
    main_window.invoke_update_default_font_size(FontSizeAction::Default);
    
    main_window.run()?;
    attempt!(attempt!(config.read()).save(&file_root));
    // WORD_PROVIDER.stop();
    Ok(())
}
