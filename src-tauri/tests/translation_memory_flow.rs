#[path="support/legacy_schema.rs"] mod legacy_schema;
#[path="support/empty_credentials.rs"] mod empty_credentials;
use local_film_library_preview_lib::{catalog, reset, source_scan::{SourceScanSession,change_source_state}, storage::Storage, translation};
use std::{fs, path::{Path, PathBuf}};

fn fixture(title: &str) -> (tempfile::TempDir, Storage, String) {
    let home=tempfile::tempdir().unwrap();
    let storage=Storage::open(&home.path().join("data")).unwrap();
    storage.with_transaction(|tx| {
        tx.execute("INSERT INTO libraries(id,display_name,root_path) VALUES('source','合成片库','X:/synthetic')",[])?;
        tx.execute("INSERT INTO films(id) VALUES('film')",[])?;
        tx.execute("INSERT INTO file_observations(id,library_id,film_id,observed_path,kind) VALUES('video','source','film',?1,'video')",[format!("{title}/synthetic.mp4")])?;
        Ok(())
    }).unwrap();
    (home,storage,"film".into())
}
fn translate(storage:&Storage,id:&str,text:&str) {
    let prepared=translation::prepare_translation(storage,id,false).unwrap().unwrap();
    translation::save_translation(storage,&prepared,"deepl",text,"JA").unwrap();
}
fn change(storage:&Storage,title:&str) {
    storage.with_connection(|db|db.execute("UPDATE file_observations SET observed_path=?1 WHERE id='video'",[format!("{title}/synthetic.mp4")])).unwrap();
}
fn film(storage:&Storage)->catalog::CatalogFilm { catalog::list_films(storage).unwrap().remove(0) }
fn memory_rows(storage: &Storage) -> Vec<Vec<String>> {
    storage.with_connection(|db| {
        let mut query = db.prepare("SELECT id,identity_anchor,normalized_code,original_title,normalized_title,body,fingerprint,source_language,translated_title,provider,translated_at,last_matched_at FROM translation_memory ORDER BY id")?;
        let rows = query.query_map([], |row| (0..12).map(|column| row.get(column)).collect::<rusqlite::Result<Vec<String>>>())?;
        rows.collect()
    }).unwrap()
}
fn scan_new_source(storage: &Storage, root: &Path) -> String {
    let session = SourceScanSession::new();
    let preview = session.preview(storage, root, None).unwrap();
    session.confirm(storage, &preview.token.unwrap()).unwrap();
    film(storage).id
}
fn real_fixture(title: &str) -> (tempfile::TempDir, Storage, PathBuf, String) {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("synthetic-media");
    fs::create_dir_all(root.join(title)).unwrap();
    fs::write(root.join(title).join("unique-synthetic.mp4"), b"synthetic original media").unwrap();
    let storage = Storage::open(&home.path().join("data")).unwrap();
    let id = scan_new_source(&storage, &root);
    (home, storage, root, id)
}
// Legacy title field remains backup-compatible; new editing writes translation memory.
fn legacy_title(storage:&Storage,id:&str,title:&str) {
    storage.with_connection(|db|db.execute("INSERT INTO film_manual(film_id,title) VALUES(?1,?2) ON CONFLICT(film_id) DO UPDATE SET title=excluded.title",rusqlite::params![id,title])).unwrap();
}

#[test]
fn automatic_title_survives_restart_and_raw_title_remains() {
    let (home,storage,id)=fixture("[ABP-123] 美女教師と秘密の放課後");
    translate(&storage,&id,"美女教师与秘密的放学后");
    let reopened=Storage::open(&home.path().join("data")).unwrap();
    let saved=film(&reopened);
    assert_eq!(saved.title,"美女教师与秘密的放学后");
    assert_eq!(saved.original_title,"[ABP-123] 美女教師と秘密の放課後");
    assert!(!saved.translation_stale);
}

#[test]
fn legacy_title_does_not_override_translation_or_block_retranslation() {
    let (_home,storage,id)=fixture("[ABP-123] 放課後の先生");
    translate(&storage,&id,"自动教师");
    catalog::set_rating(&storage,&id,Some(4)).unwrap();
    legacy_title(&storage,&id,"人工教师");
    assert_eq!(film(&storage).title,"自动教师");
    assert!(translation::prepare_translation(&storage,&id,true).unwrap().is_some());
    assert_eq!(film(&storage).title,"自动教师");
    assert_eq!(film(&storage).rating,Some(4.0));
}

#[test]
fn restore_initial_state_preserves_complete_automatic_memory_for_strong_reuse_after_active_scan() {
    let (_home,storage,root,id)=real_fixture("[ABP-123] 放課後の先生");
    translate(&storage,&id,"自动教师");
    let remembered = memory_rows(&storage);
    reset::restore_initial_state_with(&storage, &mut empty_credentials::EmptyCredentials::default()).unwrap();
    assert!(catalog::list_films(&storage).unwrap().is_empty());
    assert_eq!(memory_rows(&storage), remembered);
    assert_eq!(translation::stats(&storage).unwrap().memory,1);
    let new_id = scan_new_source(&storage, &root);
    assert_ne!(new_id, id);
    assert_eq!(film(&storage).title,"自动教师");
    assert!(translation::prepare_translation(&storage, &new_id, false).unwrap().is_none());
    assert_eq!(fs::read(root.join("[ABP-123] 放課後の先生/unique-synthetic.mp4")).unwrap(), b"synthetic original media");
}

#[test]
fn independent_clear_preserves_rating_and_original_ignores_legacy_title() {
    let (_home,storage,id)=fixture("[ABP-123] 放課後の先生");
    translate(&storage,&id,"自动教师");
    legacy_title(&storage,&id,"人工教师");
    catalog::set_rating(&storage,&id,Some(5)).unwrap();
    assert_eq!(translation::clear_memory(&storage).unwrap(),1);
    assert_eq!(translation::stats(&storage).unwrap().memory,0);
    let saved=film(&storage);
    assert_ne!(saved.title,"人工教师");
    assert_eq!(saved.automatic_title,None);
    assert_eq!(saved.rating,Some(5.0));
    assert_eq!(saved.original_title,"[ABP-123] 放課後の先生");
}

#[test]
fn memory_clear_failure_rolls_back_links_and_memory() {
    let (_home,storage,id)=fixture("[ABP-123] 放課後の先生");
    translate(&storage,&id,"自动教师");
    storage.with_connection(|db|db.execute_batch("CREATE TRIGGER reject_memory_clear BEFORE DELETE ON translation_memory BEGIN SELECT RAISE(ABORT,'synthetic'); END;")).unwrap();
    assert!(translation::clear_memory(&storage).is_err());
    assert_eq!(translation::stats(&storage).unwrap().memory,1);
    assert_eq!(film(&storage).title,"自动教师");
}

#[test]
fn backup_contains_actual_translation_rows_and_restores_them() {
    let (home,storage,id)=fixture("[ABP-123] 放課後の先生");
    translate(&storage,&id,"历史教师");
    legacy_title(&storage,&id,"备份人工标题");
    let backup=storage.create_backup().unwrap();
    let db=rusqlite::Connection::open(home.path().join(format!("data/backups/backup-{backup}.sqlite3"))).unwrap();
    assert_eq!(db.query_row("SELECT translated_title FROM translation_memory",[],|r|r.get::<_,String>(0)).unwrap(),"历史教师");
    drop(db);
    translation::clear_memory(&storage).unwrap();
    legacy_title(&storage,&id,"修改后的人工标题");
    storage.restore_backup(&backup).unwrap();
    assert_eq!(film(&storage).title,"历史教师");
    assert_eq!(storage.with_connection(|db|db.query_row("SELECT title FROM film_manual WHERE film_id='film'",[],|r|r.get::<_,String>(0))).unwrap(),"备份人工标题");
    assert_eq!(film(&storage).automatic_title.as_deref(),Some("历史教师"));
}

#[test]
fn deleting_current_film_keeps_memory_and_strong_new_identity_reuses_it() {
    let (_home,storage,id)=fixture("[ABP-123] 放課後の先生");
    translate(&storage,&id,"历史教师");
    storage.with_transaction(|tx| {
        tx.execute("DELETE FROM file_observations",[])?;
        tx.execute("DELETE FROM films WHERE id='film'",[])?;
        tx.execute("INSERT INTO films(id) VALUES('new-film')",[])?;
        tx.execute("INSERT INTO file_observations(id,library_id,film_id,observed_path,kind) VALUES('new-video','source','new-film','[ABP-123] 放課後の先生/new-name.mp4','video')",[])?;
        Ok(())
    }).unwrap();
    assert_eq!(translation::stats(&storage).unwrap().memory,1);
    assert_eq!(film(&storage).title,"历史教师");
    assert!(translation::prepare_translation(&storage,"new-film",false).unwrap().is_none());
}

#[test]
fn known_noise_and_code_case_do_not_require_another_api_request() {
    let (_home,storage,id)=fixture("[ABP-123] 美女教師と秘密の放課後");
    translate(&storage,&id,"历史教师");
    for title in [" [abp-123] 美女教師と秘密の放課後 [4K] ","[ABP-123] [ABP-123] 美女教師と秘密の放課後【中文字幕】","[ABP-123] 美女教師と秘密の放課後！"] {
        change(&storage,title);
        let saved=film(&storage);
        assert_eq!(saved.title,"历史教师");
        assert!(!saved.translation_stale,"{title}");
        assert!(translation::prepare_translation(&storage,&id,false).unwrap().is_none());
    }
}

#[test]
fn changed_body_keeps_old_translation_stale_without_automatic_retranslation() {
    let (_home,storage,id)=fixture("[ABP-123] 放課後の先生");
    translate(&storage,&id,"历史教师");
    change(&storage,"[ABP-123] 放課後の先生 完全版");
    assert_eq!(film(&storage).title,"历史教师");
    assert!(film(&storage).translation_stale);
    assert!(translation::prepare_translation(&storage,&id,false).unwrap().is_none());
    assert!(translation::prepare_translation(&storage,&id,true).unwrap().is_some());
}

#[test]
fn bracketed_metadata_is_not_part_of_translation_body() {
    let (_home,storage,id)=fixture("[ABP-123] 放課後の先生【通常版】");
    translate(&storage,&id,"历史教师");
    change(&storage,"[ABP-123] 放課後の先生【特別版】");
    assert!(!film(&storage).translation_stale);
}

#[test]
fn same_code_different_title_does_not_reattach_other_film_memory() {
    let (_home,storage,id)=fixture("[ABP-123] 放課後の先生");
    translate(&storage,&id,"历史教师");
    storage.with_transaction(|tx| {
        tx.execute("INSERT INTO films(id) VALUES('different')",[])?;
        tx.execute("INSERT INTO file_observations(id,library_id,film_id,observed_path,kind) VALUES('different-video','source','different','[ABP-123] 海にいる猫/different.mp4','video')",[])?;
        Ok(())
    }).unwrap();
    let other=catalog::list_films(&storage).unwrap().into_iter().find(|f|f.id=="different").unwrap();
    assert_eq!(other.automatic_title,None);
    assert!(translation::prepare_translation(&storage,"different",false).unwrap().is_some());
}

#[test]
fn no_code_requires_same_stable_film_evidence() {
    let (_home,storage,id)=fixture("放課後の先生");
    translate(&storage,&id,"历史教师");
    storage.with_transaction(|tx| {
        tx.execute("DELETE FROM film_translations",[])?;
        tx.execute("INSERT INTO films(id) VALUES('other')",[])?;
        tx.execute("INSERT INTO file_observations(id,library_id,film_id,observed_path,kind) VALUES('other-video','source','other','放課後の先生/other.mp4','video')",[])?;
        Ok(())
    }).unwrap();
    let films=catalog::list_films(&storage).unwrap();
    assert_eq!(films.iter().find(|f|f.id==id).unwrap().title,"历史教师");
    assert_eq!(films.iter().find(|f|f.id=="other").unwrap().automatic_title,None);
}

#[test]
fn switch_provider_retains_memory_and_does_not_create_requests() {
    let (_home,storage,id)=fixture("[ABP-123] 放課後の先生");
    translate(&storage,&id,"历史教师");
    let mut settings=translation::load_settings(&storage).unwrap();
    settings.provider="google".into();settings.enabled=true;
    translation::save_settings(&storage,&settings).unwrap();
    assert_eq!(translation::preview(&storage).unwrap().summary.api_requests,0);
    assert!(translation::prepare_translation(&storage,&id,false).unwrap().is_none());
}

#[test]
fn title_changes_in_flight_reject_result_without_destroying_existing_memory() {
    let (_home,storage,id)=fixture("[ABP-123] 放課後の先生");
    let prepared=translation::prepare_translation(&storage,&id,false).unwrap().unwrap();
    change(&storage,"[ABP-123] 海にいる猫");
    assert_eq!(translation::save_translation(&storage,&prepared,"google","错误旧结果","JA").unwrap_err(),"translation_changed");
    assert_eq!(translation::stats(&storage).unwrap().memory,0);
    assert_eq!(film(&storage).automatic_title,None);
}

#[test]
fn legacy_title_does_not_block_network_result() {
    let (_home,storage,id)=fixture("[ABP-123] 放課後の先生");
    let prepared=translation::prepare_translation(&storage,&id,false).unwrap().unwrap();
    legacy_title(&storage,&id,"人工教师");
    translation::save_translation(&storage,&prepared,"deepl","自动教师","JA").unwrap();
    assert_eq!(film(&storage).title,"自动教师");
    assert_eq!(translation::stats(&storage).unwrap().memory,1);
}

#[test]
fn disabled_source_and_dead_alias_are_not_candidates() {
    let (_home,storage,id)=fixture("[ABP-123] 放課後の先生");
    storage.with_connection(|db|db.execute("UPDATE libraries SET enabled=0",[])).unwrap();
    assert!(translation::preview(&storage).unwrap().entries.is_empty());
    assert!(translation::prepare_translation(&storage,&id,false).is_err());
    storage.with_transaction(|tx| {tx.execute("UPDATE libraries SET enabled=1",[])?;tx.execute("INSERT INTO films(id) VALUES('alias')",[])?;tx.execute("INSERT INTO film_aliases(id,film_id) VALUES('alias','film')",[])?;Ok(())}).unwrap();
    assert!(translation::prepare_translation(&storage,"alias",false).is_err());
}

#[test]
fn preview_counts_unique_requests_but_preserves_distinct_film_identity() {
    let (_home,storage,_id)=fixture("[ABP-123] 放課後の先生");
    storage.with_transaction(|tx| {
        tx.execute("INSERT INTO films(id) VALUES('other')",[])?;
        tx.execute("INSERT INTO file_observations(id,library_id,film_id,observed_path,kind) VALUES('other-video','source','other','[OTHER-456] 放課後の先生/other.mp4','video')",[])?;Ok(())
    }).unwrap();
    let preview=translation::preview(&storage).unwrap();
    assert_eq!(preview.summary.pending,2);
    assert_eq!(preview.summary.api_requests,1);
    assert_eq!(catalog::list_films(&storage).unwrap().len(),2);
}

#[test]
fn han_only_titles_are_uncertain_candidates_for_explicit_selection() {
    let (_home,storage,id)=fixture("[ABP-123] 美女教師秘密放課後");
    let preview=translation::preview(&storage).unwrap();
    assert_eq!(preview.entries[0].status,"language_uncertain");
    let prepared=translation::prepare_translation(&storage,&id,false).unwrap().unwrap();
    assert_eq!(prepared.body,"美女教師秘密放課後");
    assert_eq!(film(&storage).automatic_title,None);
}

#[test]
fn placeholder_and_code_only_titles_do_not_become_api_candidates() {
    let (_home,storage,id)=fixture("[ABP-123]");
    assert_eq!(translation::preview(&storage).unwrap().summary.api_requests,0);
    assert!(translation::prepare_translation(&storage,&id,false).unwrap().is_none());
}

#[test]
fn settings_validate_nonsecret_fields_and_backup_only_contains_nonsecret_metadata() {
    let (home,storage,_id)=fixture("[ABP-123] 放課後の先生");
    let mut settings=translation::load_settings(&storage).unwrap();
    settings.provider="secret-as-provider".into();
    assert!(translation::save_settings(&storage,&settings).is_err());
    settings.provider="google".into();settings.enabled=true;
    translation::save_settings(&storage,&settings).unwrap();
    let backup=storage.create_backup().unwrap();
    let db=rusqlite::Connection::open(home.path().join(format!("data/backups/backup-{backup}.sqlite3"))).unwrap();
    let names:Vec<String>=db.prepare("SELECT name FROM pragma_table_info('translation_settings') ORDER BY cid").unwrap().query_map([],|r|r.get(0)).unwrap().collect::<rusqlite::Result<_>>().unwrap();
    assert_eq!(names,["singleton","enabled","provider","deepl_plan","target_language","azure_region","baidu_app_id"]);
    assert_eq!(db.query_row("SELECT provider FROM translation_settings",[],|r|r.get::<_,String>(0)).unwrap(),"google");
}

#[test]
fn real_synthetic_source_removal_readd_and_rescan_keep_translation_memory() {
    let home=tempfile::tempdir().unwrap();let root=home.path().join("synthetic-media");
    let directory=root.join("[ABP-123] 放課後の先生");std::fs::create_dir_all(&directory).unwrap();std::fs::write(directory.join("unique-synthetic.mp4"),b"synthetic").unwrap();
    let storage=Storage::open(&home.path().join("data")).unwrap();let session=SourceScanSession::new();
    let add=|session:&SourceScanSession| {let p=session.preview(&storage,&root,None).unwrap();session.confirm(&storage,&p.token.unwrap()).unwrap().library_id};
    let source=add(&session);let id=film(&storage).id;
    translate(&storage,&id,"历史教师");legacy_title(&storage,&id,"人工教师");
    change_source_state(&storage,&source,"remove").unwrap();
    assert!(catalog::list_films(&storage).unwrap().is_empty());assert_eq!(translation::stats(&storage).unwrap().memory,1);
    let new_source=add(&session);let saved=film(&storage);
    assert_eq!(saved.id,id);assert_eq!(saved.title,"历史教师");assert_eq!(saved.automatic_title.as_deref(),Some("历史教师"));
    let p=session.preview(&storage,&root,Some(&new_source)).unwrap();session.confirm(&storage,&p.token.unwrap()).unwrap();
    assert_eq!(film(&storage).title,"历史教师");
    assert_eq!(std::fs::read(directory.join("unique-synthetic.mp4")).unwrap(),b"synthetic");
}

#[test]
fn schema10_migration_and_old_backup_restore_keep_actual_manual_values() {
    let (home,storage,id)=fixture("[ABP-123] 放課後の先生");
    catalog::set_rating(&storage,&id,Some(4)).unwrap();
    storage.with_connection(|db|{legacy_schema::remove_actor_v17(db)?; db.execute_batch("INSERT INTO film_manual(film_id,title) VALUES('film','旧版人工片名') ON CONFLICT(film_id) DO UPDATE SET title=excluded.title; DROP TABLE film_translations; DROP TABLE translation_memory; DROP TABLE translation_settings; PRAGMA user_version=10;")}).unwrap();
    let backup=storage.create_backup().unwrap();
    drop(storage);
    let storage=Storage::open(&home.path().join("data")).unwrap();
    assert_eq!(storage.schema_version().unwrap(), 23);
    assert_ne!(film(&storage).title,"旧版人工片名");
    assert_eq!(film(&storage).rating,Some(4.0));
    assert_eq!(storage.list_protections().unwrap().len(),1);
    legacy_title(&storage,&id,"新版人工片名");
    storage.restore_backup(&backup).unwrap();
    assert_eq!(storage.schema_version().unwrap(), 23);
    assert_ne!(film(&storage).title,"旧版人工片名");
    assert_eq!(storage.with_connection(|db|db.query_row("SELECT title FROM film_manual WHERE film_id='film'",[],|r|r.get::<_,String>(0))).unwrap(),"旧版人工片名");
    assert_eq!(film(&storage).rating,Some(4.0));
    assert_eq!(translation::stats(&storage).unwrap().memory,0);
}

#[test]
fn schema10_migration_protection_failure_keeps_database_unmodified() {
    let (home,storage,id)=fixture("[ABP-123] 放課後の先生");
    catalog::set_rating(&storage,&id,Some(3)).unwrap();
    storage.with_connection(|db|{legacy_schema::remove_actor_v17(db)?; db.execute_batch("DROP TABLE film_translations; DROP TABLE translation_memory; DROP TABLE translation_settings; PRAGMA user_version=10;")}).unwrap();
    drop(storage);
    std::fs::write(home.path().join("data/protections"),b"synthetic blocking file").unwrap();
    assert!(Storage::open(&home.path().join("data")).is_err());
    let db=rusqlite::Connection::open(home.path().join("data/library.sqlite3")).unwrap();
    assert_eq!(db.pragma_query_value::<i64,_>(None,"user_version",|r|r.get(0)).unwrap(),10);
    assert_eq!(db.query_row("SELECT rating FROM film_manual WHERE film_id='film'",[],|r|r.get::<_,i64>(0)).unwrap(),6);
    assert_eq!(db.query_row("SELECT count(*) FROM sqlite_master WHERE name='translation_memory'",[],|r|r.get::<_,i64>(0)).unwrap(),0);
}

#[test]
fn schema11_backup_missing_translation_table_is_rejected_before_replacement() {
    let (home,storage,id)=fixture("[ABP-123] 放課後の先生");
    translate(&storage,&id,"保留自动教师");
    let backup=storage.create_backup().unwrap();
    let db=rusqlite::Connection::open(home.path().join(format!("data/backups/backup-{backup}.sqlite3"))).unwrap();
    db.execute_batch("DROP TABLE translation_settings;").unwrap();drop(db);
    assert!(storage.restore_backup(&backup).is_err());
    assert_eq!(film(&storage).title,"保留自动教师");
    assert_eq!(translation::stats(&storage).unwrap().memory,1);
}

#[test]
fn disabled_source_in_flight_rejects_commit_but_keeps_film_observations() {
    let (_home,storage,id)=fixture("[ABP-123] 放課後の先生");
    let prepared=translation::prepare_translation(&storage,&id,false).unwrap().unwrap();
    storage.with_connection(|db|db.execute("UPDATE libraries SET enabled=0",[])).unwrap();
    assert_eq!(translation::save_translation(&storage,&prepared,"deepl","迟到结果","JA").unwrap_err(),"translation_changed");
    assert_eq!(storage.with_connection(|db|db.query_row("SELECT count(*) FROM file_observations",[],|r|r.get::<_,i64>(0))).unwrap(),1);
    assert_eq!(translation::stats(&storage).unwrap().memory,0);
}

#[test]
fn explicit_retranslation_is_the_latest_memory_after_whole_film_reentry() {
    let (_home,storage,id)=fixture("[ABP-123] 放課後の先生");
    translate(&storage,&id,"旧译名");
    let prepared=translation::prepare_translation(&storage,&id,true).unwrap().unwrap();
    translation::save_translation(&storage,&prepared,"google","新译名","ja").unwrap();
    storage.with_transaction(|tx| {
        tx.execute("DELETE FROM file_observations",[])?;
        tx.execute("DELETE FROM films",[])?;
        tx.execute("INSERT INTO films(id) VALUES('returned')",[])?;
        tx.execute("INSERT INTO file_observations(id,library_id,film_id,observed_path,kind) VALUES('returned-video','source','returned','[ABP-123] 放課後の先生/synthetic.mp4','video')",[])?;
        Ok(())
    }).unwrap();
    assert_eq!(film(&storage).automatic_title.as_deref(),Some("新译名"));
    assert!(translation::prepare_translation(&storage,"returned",false).unwrap().is_none());
}

#[test]
fn independent_clear_rechecks_confirmation_count_and_leaves_values_on_mismatch() {
    let (_home,storage,id)=fixture("[ABP-123] 放課後の先生");
    translate(&storage,&id,"保留的自动译名");
    assert_eq!(translation::clear_memory_expected(&storage,0).unwrap_err(),"translation_memory_changed");
    assert_eq!(translation::stats(&storage).unwrap().memory,1);
    assert_eq!(film(&storage).automatic_title.as_deref(),Some("保留的自动译名"));
    assert_eq!(translation::clear_memory_expected(&storage,1).unwrap(),1);
    assert!(film(&storage).automatic_title.is_none());
}

#[test]
fn new_provider_settings_and_titles_survive_backup_restore_without_credentials() {
 let (_home,storage,id)=fixture("[ABC-001] fictional title");
 for provider in ["azure","baidu"] {
  let mut settings=translation::load_settings(&storage).unwrap();settings.provider=provider.into();settings.azure_region="eastasia".into();settings.baidu_app_id="123456789".into();settings.enabled=true;
  translation::save_settings(&storage,&settings).unwrap();
  let prepared=translation::prepare_translation(&storage,&id,true).unwrap().unwrap();translation::save_translation(&storage,&prepared,provider,"合成译文","en").unwrap();
  let backup=storage.create_backup().unwrap();settings.azure_region="global".into();settings.baidu_app_id="987654321".into();translation::save_settings(&storage,&settings).unwrap();
  storage.restore_backup(&backup).unwrap();let restored=translation::load_settings(&storage).unwrap();assert_eq!(restored.provider,provider);assert_eq!(restored.azure_region,"eastasia");assert_eq!(restored.baidu_app_id,"123456789");assert_eq!(film(&storage).title,"合成译文");
 }
}

#[test]
fn edited_title_persists_as_memory_and_rejects_an_in_flight_api_result() {
    let (home,storage,root,id)=real_fixture("[ABP-123] Fictional original【metadata】");
    let pending=translation::prepare_translation(&storage,&id,false).unwrap().unwrap();
    assert_eq!(pending.body,"Fictional original");
    let saved=translation::edit_title(&storage,&id,&film(&storage).original_title,&film(&storage).title,"  用户片名  ").unwrap();
    assert_eq!(saved.title,"用户片名");
    assert_eq!(translation::save_translation(&storage,&pending,"deepl","迟到译名","EN").unwrap_err(),"translation_changed");
    assert!(translation::prepare_translation(&storage,&id,true).unwrap().is_none());
    let reopened=Storage::open(&home.path().join("data")).unwrap();
    assert_eq!(film(&reopened).title,"用户片名");
    let backup=reopened.create_backup().unwrap();
    translation::edit_title(&reopened,&id,&film(&reopened).original_title,"用户片名","修改后").unwrap();
    reopened.restore_backup(&backup).unwrap();
    assert_eq!(film(&reopened).title,"用户片名");
    let remembered = memory_rows(&reopened);
    reset::restore_initial_state_with(&reopened, &mut empty_credentials::EmptyCredentials::default()).unwrap();
    assert!(catalog::list_films(&reopened).unwrap().is_empty());
    assert_eq!(memory_rows(&reopened), remembered);
    let new_id = scan_new_source(&reopened, &root);
    assert_ne!(new_id, id);
    let current = film(&reopened);
    assert_eq!(current.title, "Fictional original");
    assert!(!current.title_edited);
    assert_eq!(current.automatic_title, None);
    assert!(translation::prepare_translation(&reopened, &new_id, false).unwrap().is_some());
    assert_eq!(reopened.with_connection(|db| db.query_row("SELECT COUNT(*) FROM film_translations", [], |r| r.get::<_, i64>(0))).unwrap(), 0);
    assert_eq!(memory_rows(&reopened), remembered);
}

#[test]
fn restore_initial_state_preserves_complete_original_choice_memory_without_reapplying_to_new_identity() {
    let (_home, storage, root, id) = real_fixture("[ABP-123] Fictional original【metadata】");
    let current = film(&storage);
    translation::edit_title(&storage, &id, &current.original_title, &current.title, "").unwrap();
    let remembered = memory_rows(&storage);
    assert_eq!(remembered.len(), 1);
    assert_eq!(remembered[0][9], "original");
    reset::restore_initial_state_with(&storage, &mut empty_credentials::EmptyCredentials::default()).unwrap();
    assert!(catalog::list_films(&storage).unwrap().is_empty());
    assert_eq!(memory_rows(&storage), remembered);
    let new_id = scan_new_source(&storage, &root);
    assert_ne!(new_id, id);
    let current = film(&storage);
    assert_eq!(current.title, "Fictional original");
    assert!(!current.title_edited);
    assert_eq!(current.automatic_title, None);
    assert!(translation::prepare_translation(&storage, &new_id, false).unwrap().is_some());
    assert_eq!(storage.with_connection(|db| db.query_row("SELECT COUNT(*) FROM film_translations", [], |r| r.get::<_, i64>(0))).unwrap(), 0);
    assert_eq!(memory_rows(&storage), remembered);
}

#[test]
fn clearing_title_restores_clean_original_without_reviving_old_translation() {
    let (_home,storage,id)=fixture("[ABP-123] Fictional original【metadata】");
    translate(&storage,&id,"自动译名");
    let pending=translation::prepare_translation(&storage,&id,true).unwrap().unwrap();
    translation::edit_title(&storage,&id,&film(&storage).original_title,"自动译名","用户片名").unwrap();
    let saved=translation::edit_title(&storage,&id,&film(&storage).original_title,"用户片名","   ").unwrap();
    assert_eq!(saved.title,"Fictional original");
    assert_eq!(saved.automatic_title,None);
    assert!(translation::save_translation(&storage,&pending,"deepl","迟到译名","EN").is_err());
    assert_eq!(film(&storage).title,"Fictional original");
    change(&storage,"[ABP-123] New original【more metadata】");
    assert_eq!(film(&storage).title,"New original");
    assert!(translation::prepare_translation(&storage,&id,false).unwrap().is_some());
}

#[test]
fn editing_shared_translation_changes_only_target_and_stale_editor_is_rejected() {
    let (_home,storage,id)=fixture("[ABP-123] Fictional original");
    translate(&storage,&id,"共同译名");
    storage.with_transaction(|tx| {
        tx.execute("INSERT INTO films(id) VALUES('other')",[])?;
        tx.execute("INSERT INTO file_observations(id,library_id,film_id,observed_path,kind) VALUES('other-video','source','other','[ABP-123] Fictional original/other.mp4','video')",[])?;
        Ok(())
    }).unwrap();
    assert!(catalog::list_films(&storage).unwrap().iter().all(|f|f.title=="共同译名"));
    let original=film(&storage).original_title;
    translation::edit_title(&storage,&id,&original,"共同译名","只修改此片").unwrap();
    let other=catalog::list_films(&storage).unwrap().into_iter().find(|f|f.id=="other").unwrap();
    assert_eq!(other.title,"共同译名");
    assert_eq!(translation::edit_title(&storage,&id,&original,"共同译名","旧编辑器").unwrap_err(),"translation_changed");
    assert_eq!(film(&storage).title,"只修改此片");
    translation::clear_memory(&storage).unwrap();
    assert_eq!(film(&storage).title,"Fictional original");
}


#[test]
fn edited_title_survives_source_remove_readd_and_uses_latest_scoped_memory() {
    let home=tempfile::tempdir().unwrap();let root=home.path().join("synthetic-media");
    let directory=root.join("[QA-101] Fictional title【metadata】");std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("unique-qa.mp4"),b"synthetic").unwrap();
    let storage=Storage::open(&home.path().join("data")).unwrap();let session=SourceScanSession::new();
    let add=||{let p=session.preview(&storage,&root,None).unwrap();session.confirm(&storage,&p.token.unwrap()).unwrap().library_id};
    let source=add();let current=film(&storage);
    translation::edit_title(&storage,&current.id,&current.original_title,&current.title,"第一片名").unwrap();
    translation::edit_title(&storage,&current.id,&current.original_title,"第一片名","第二片名").unwrap();
    change_source_state(&storage,&source,"remove").unwrap();assert!(catalog::list_films(&storage).unwrap().is_empty());
    add();assert_eq!(film(&storage).id,current.id);assert_eq!(film(&storage).title,"第二片名");
    storage.with_connection(|db|db.execute("DELETE FROM film_translations",[])).unwrap();
    assert_eq!(film(&storage).title,"第二片名");
    translation::edit_title(&storage,&current.id,&current.original_title,"第二片名","").unwrap();
    storage.with_connection(|db|db.execute("DELETE FROM film_translations",[])).unwrap();
    assert_eq!(film(&storage).title,"Fictional title");
    assert_eq!(storage.with_connection(|db|db.query_row("SELECT count(*) FROM film_translations",[],|r|r.get::<_,usize>(0))).unwrap(),1);
    assert_eq!(std::fs::read(directory.join("unique-qa.mp4")).unwrap(),b"synthetic");
}

#[test]
fn editing_rolls_back_both_memory_and_link_on_failure_and_rejects_changed_original() {
    let (_home,storage,id)=fixture("[QA-101] Fictional original");translate(&storage,&id,"原译名");
    let before=translation::stats(&storage).unwrap().memory;
    storage.with_connection(|db|db.execute_batch("CREATE TRIGGER reject_title BEFORE UPDATE ON film_translations WHEN NEW.memory_id<>OLD.memory_id BEGIN SELECT RAISE(ABORT,'synthetic'); END;")).unwrap();
    let original=film(&storage).original_title;
    assert!(translation::edit_title(&storage,&id,&original,"原译名","新片名").is_err());
    storage.with_connection(|db|db.execute_batch("DROP TRIGGER reject_title;")).unwrap();
    assert_eq!(film(&storage).title,"原译名");assert_eq!(translation::stats(&storage).unwrap().memory,before);
    change(&storage,"[QA-101] Changed original");
    assert_eq!(translation::edit_title(&storage,&id,&original,"原译名","新片名").unwrap_err(),"translation_changed");
    assert_eq!(film(&storage).title,"原译名");
}
