#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
slint::include_modules!();
use slint::Model;

use std::rc::Rc;
use std::sync::{LazyLock, RwLock, Arc};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{JoinHandle, self};
use std::time::{Instant, Duration};

mod search;
use libbass::db::{self, Music as DBMusic, Keyword};


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
    // println!("words called");
    let hint = hint.to_string();
    let words = WORD_PROVIDER.words();
    let keys: Vec<slint::SharedString> = words.into_iter().filter_map(|k| {
        (k.category.as_ref().map_or(false, |c| c.starts_with(&hint)) || k.keyword.starts_with(&hint))
            .then(|| k.to_string().into())
    }).collect();
    Rc::new(slint::VecModel::from(keys)).into()
}

fn main() -> Result<(), slint::PlatformError> {
    db::init(Some("db.sqlite3")).unwrap();
    
    let main_window = Bass::new()?;
    let add_dialog = AddDialog::new()?;
    let search_dialog = SearchDialog::new()?;

    main_window.global::<KeywordInputLogic>().on_words(words_by_hint);
    add_dialog.global::<KeywordInputLogic>().on_words(words_by_hint);
    search_dialog.global::<KeywordInputLogic>().on_words(words_by_hint);

    add_dialog.on_validate_time(|t| {
        // This seems to work, but doing it as a global with identical code seems to not, for some
        // reason.
        // This reason is that "globals" aren't technically global, and instead are essentially
        // "instanced" per window.
        t.chars().all(|c| c.is_ascii_digit() || c == ':') && t.split(":").all(|s| {
            (s.len() == 1 || s.len() == 2) && s.parse::<u16>().unwrap() < 60
        })
    });
    let weak_add = add_dialog.as_weak();
    add_dialog.on_cancel_clicked(move || {
        let dialog = weak_add.unwrap();
        dialog.hide().unwrap();
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
        music.insert_with_keywords(&mut keywords).unwrap();
        main_window.invoke_trigger_refresh();
        let _ = dialog.hide();
    });

    
    // let model = Rc::new(slint::VecModel::from_iter(music_list));
    // main_window.set_music_list(model.into());

    let weak_add = add_dialog.as_weak();
    main_window.on_show_add_dialog(move || {
        let dialog = weak_add.unwrap();
        dialog.invoke_clear_form();
        dialog.show().unwrap();
    });

    main_window.on_update_entry(move |m| {
        let mut music = music_from_ui(&m);
        let mut keywords: Vec<Keyword> = m.keywords.iter().map(|k| k.parse().unwrap()).collect();

        let _ = music.update_keywords(&mut keywords); // TODO Error handling
        let _ = music.insert();
    });

    main_window.on_remove_keyword(move |m, idx, key| {
        let mut music = music_from_ui(&m);
        let mut key = key.parse().unwrap();
        let _ = music.remove_keyword(&mut key).unwrap(); // TODO Error handling

        let keys = m.keywords.as_any().downcast_ref::<slint::VecModel<slint::SharedString>>().unwrap();
        keys.remove(idx as usize);
    });

    main_window.on_remove_entry(move |m| {
        let music = music_from_ui(&m);
        let _ = music.delete(); // TODO Error handling
    });

    main_window.on_add_keyword(move |m, k| {
        let mut music = music_from_ui(&m);
        let mut key = k.parse().unwrap();
        if !music.keywords().unwrap().unwrap().contains(&key) {
            let _ = music.add_keyword(&mut key).unwrap(); // TODO Error handling;

            let keys = m.keywords.as_any().downcast_ref::<slint::VecModel<slint::SharedString>>().unwrap();
            keys.push(k);
        }
    });

    let weak_search = search_dialog.as_weak();
    main_window.on_show_search_dialog(move || {
        let dialog = weak_search.unwrap();
        dialog.show().unwrap();

        // TODO Implement dialog

        // let search = Search::new(Field::Title, SearchOp::Contains, "a", true);
        // let search = UISearch {
        //     name: "TODO".into(),
        //     search_text: search.to_string().into(),
        // };
        // *CURRENT_SEARCH.write().unwrap() = Some(search);
        // main_window.invoke_trigger_refresh();
    });

    let weak_main = main_window.as_weak();
    main_window.on_search(move |s| {
        let main_window = weak_main.unwrap();
        *CURRENT_SEARCH.write().unwrap() = Some(s);
        main_window.invoke_trigger_refresh();
    });

    let weak_main = main_window.as_weak();
    main_window.on_clear_search(move || {
        let main_window = weak_main.unwrap();
        *CURRENT_SEARCH.write().unwrap() = None;
        main_window.invoke_trigger_refresh();
    });

    let weak_main = main_window.as_weak();
    main_window.on_trigger_refresh(move || {
        let main_window = weak_main.unwrap();

        let musics = match *CURRENT_SEARCH.read().unwrap() {
            Some(ref s) => {
                let text = s.search_text.to_string();
                let search: search::Search = text.parse().unwrap(); // TODO you know what
                search.execute().unwrap() // TODO hey what do you know?
            }
            None => {
                DBMusic::list_all().unwrap() // TODO handle this better.
            }
        };
        
        let music_list = musics.into_iter().map(|m| {
            let keywords = m.keywords().unwrap().unwrap(); // The first unwrap needs to be handled better
            let keywords: Vec<slint::SharedString> = keywords.into_iter().map(|k| k.to_string().into()).collect();
            let keymodel = Rc::new(slint::VecModel::from(keywords));
            Music {
                id: m.id().unwrap().into(),
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

    main_window.invoke_trigger_refresh();
    
    main_window.run()?;
    // WORD_PROVIDER.stop();
    Ok(())
}
