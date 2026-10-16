use local_film_library_preview_lib::{catalog, covers, lifecycle::{self, StartupSession}, source_scan::{self, SourceScanSession}, storage::Storage};
use std::{fs,path::Path};
fn add(s: &Storage, root: &Path) -> lifecycle::SourceRefreshResult {
    let session=SourceScanSession::new(); let p=session.preview(s,root,None).unwrap();
    lifecycle::confirm_source_refresh(s,&session,&p.token.unwrap()).unwrap()
}
#[test]
fn startup_reads_saved_data_but_removed_source_readd_scans_current_content() {
    let home=tempfile::tempdir().unwrap(); let root=home.path().join("synthetic-source");
    fs::create_dir_all(root.join("film")).unwrap(); fs::write(root.join("film/first.mp4"),[]).unwrap();
    let s=Storage::open(&home.path().join("software")).unwrap(); let source=add(&s,&root).source.library_id;
    let original=catalog::list_films(&s).unwrap().remove(0);
    catalog::set_rating(&s,&original.id,Some(4)).unwrap(); catalog::set_favorite(&s,&original.id,true).unwrap();
    fs::write(root.join("film/new.mkv"),[]).unwrap();
    s.with_connection(|db|db.execute_batch("CREATE TRIGGER reject_scan BEFORE INSERT ON file_observations BEGIN SELECT RAISE(ABORT,'synthetic'); END")).unwrap();
    assert_eq!(StartupSession::default().initialize(&s).unwrap().saved_sources,1);
    assert_eq!(catalog::list_films(&s).unwrap()[0].videos.len(),1);
    s.with_connection(|db|db.execute_batch("DROP TRIGGER reject_scan")).unwrap();
    source_scan::change_source_state(&s,&source,"remove").unwrap();
    assert!(source_scan::list_sources(&s).unwrap().is_empty());
    let next=add(&s,&root); assert_ne!(next.source.library_id,source);
    assert_eq!(next.source.discovered,2); let restored=catalog::list_films(&s).unwrap().remove(0);
    assert_eq!(restored.id,original.id); assert_eq!(restored.videos.len(),2); assert!(restored.favorite); assert_eq!(restored.rating,Some(4.0));
}
#[test]
fn startup_keeps_artwork_but_readding_rebuilds_it_from_latest_fixed_image() {
    let home=tempfile::tempdir().unwrap();let root=home.path().join("synthetic-source");
    fs::create_dir_all(root.join("film")).unwrap();fs::write(root.join("film/movie.mp4"),[]).unwrap();
    let cover=root.join("film/poster.jpg"); image::RgbImage::from_pixel(8,8,image::Rgb([20,30,40])).save(&cover).unwrap();
    let s=Storage::open(&home.path().join("software")).unwrap();let source=add(&s,&root).source.library_id;
    let film=catalog::list_films(&s).unwrap().remove(0);let before=covers::cached_thumbnail(&s,&film.id,false).unwrap();
    image::RgbImage::from_pixel(8,8,image::Rgb([90,10,15])).save(&cover).unwrap();
    StartupSession::default().initialize(&s).unwrap();assert_eq!(covers::cached_thumbnail(&s,&film.id,false).unwrap(),before);
    source_scan::change_source_state(&s,&source,"remove").unwrap();add(&s,&root);
    assert_ne!(covers::cached_thumbnail(&s,&film.id,false).unwrap(),before);
}
#[test]
fn failed_fresh_add_after_remove_preserves_manual_history_without_reactivating_source() {
    let home=tempfile::tempdir().unwrap();let root=home.path().join("synthetic-source");
    fs::create_dir_all(root.join("film")).unwrap();fs::write(root.join("film/movie.mp4"),[]).unwrap();
    let s=Storage::open(&home.path().join("software")).unwrap();let source=add(&s,&root).source.library_id;
    let film=catalog::list_films(&s).unwrap().remove(0);catalog::set_favorite(&s,&film.id,true).unwrap();
    source_scan::change_source_state(&s,&source,"remove").unwrap();
    s.with_connection(|db|db.execute_batch("CREATE TRIGGER reject_add BEFORE INSERT ON libraries BEGIN SELECT RAISE(ABORT,'synthetic'); END")).unwrap();
    let session=SourceScanSession::new();let preview=session.preview(&s,&root,None).unwrap();assert!(session.confirm(&s,&preview.token.unwrap()).is_err());
    assert!(source_scan::list_sources(&s).unwrap().is_empty());
    s.with_connection(|db|{assert!(db.query_row("SELECT favorite FROM film_manual WHERE film_id=?1",[&film.id],|r|r.get::<_,bool>(0))?);Ok(())}).unwrap();
    assert!(root.join("film/movie.mp4").is_file());
}
