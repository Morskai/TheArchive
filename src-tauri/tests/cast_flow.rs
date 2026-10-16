use local_film_library_preview_lib::{actors, catalog, source_scan::{SourceScanSession,change_source_state}, storage::Storage};
use std::{fs, path::{Path,PathBuf}};

fn touch(root: &Path, relative: &str) {
    let path = root.join(relative); fs::create_dir_all(path.parent().unwrap()).unwrap(); fs::write(path, []).unwrap();
}
fn scan(storage: &Storage, root: &Path, source: Option<&str>) -> String {
    let session = SourceScanSession::new();
    let preview = session.preview(storage, root, source).unwrap();
    session.confirm(storage, preview.token.as_deref().unwrap()).unwrap().library_id
}
fn fixture() -> (tempfile::TempDir,Storage,PathBuf,String,String) {
    let home = tempfile::tempdir().unwrap(); let root = home.path().join("source");
    for path in ["甲/actor.jpg","甲/影片/one.mp4","乙/actor.jpg","乙/另一片/two.mp4"] {touch(&root,path);}
    let storage = Storage::open(&home.path().join("data")).unwrap();
    let source = scan(&storage,&root,None);
    let film = catalog::list_films(&storage).unwrap().into_iter().find(|film|film.videos[0].observed_path.ends_with("one.mp4")).unwrap().id;
    storage.with_transaction(|db| {
        db.execute("INSERT INTO actors(id,observed_name) VALUES('a','甲'),('b','乙')",[])?;
        db.execute("INSERT INTO film_actors(film_id,actor_id,source) VALUES(?1,'a','manual')",[&film])?;
        Ok(())
    }).unwrap();
    (home,storage,root,source,film)
}

#[test]
fn manual_cast_stays_visible_after_rescan_and_reopen_without_observed_links() {
    let (home,storage,root,source,film) = fixture();
    actors::add_film_cast_actor(&storage,&film,"b").unwrap();
    scan(&storage,&root,Some(&source)); drop(storage);
    let storage = Storage::open(&home.path().join("data")).unwrap();
    let films = catalog::list_films(&storage).unwrap();
    let cast = &films.iter().find(|f|f.id==film).unwrap().actors;
    assert_eq!(cast.len(),2); assert!(cast.iter().all(|actor|!actor.system));
    assert_eq!(actors::list_actors(&storage).unwrap().iter().find(|actor|actor.id=="a").unwrap().film_ids,[film]);
    storage.with_connection(|db| {
        assert_eq!(db.query_row::<i64,_,_>("SELECT COUNT(*) FROM film_actors WHERE source='observation'",[],|r|r.get(0))?,0);
        Ok(())
    }).unwrap();
}

#[test]
fn cast_aliases_dedupe_saved_manual_actors() {
    let (_home,storage,_root,_source,film) = fixture();
    storage.with_connection(|db| {
        db.execute("INSERT INTO films(id) VALUES('old-alias')",[])?;
        db.execute("INSERT INTO film_aliases(id,film_id) VALUES('old-alias',?1)",[&film])?;
        db.execute("INSERT INTO film_actors(film_id,actor_id,source) VALUES('old-alias','a','manual')",[])?;
        Ok(())
    }).unwrap();
    let films = catalog::list_films(&storage).unwrap();
    let cast = &films.iter().find(|f|f.id==film).unwrap().actors;
    assert_eq!(cast.len(),1); assert_eq!(cast[0].id,"a"); assert!(!cast[0].system);
}

#[test]
fn cast_add_remove_is_idempotent_and_persisted() {
    let (home,storage,root,source,film) = fixture();
    assert_eq!(actors::list_cast_candidates(&storage,&film).unwrap().len(),2);
    assert_eq!(actors::add_film_cast_actor(&storage,&film,"b").unwrap().len(),2);
    assert_eq!(actors::add_film_cast_actor(&storage,&film,"b").unwrap().len(),2);
    actors::add_film_cast_actor(&storage,&film,"a").unwrap();
    assert_eq!(actors::remove_film_cast_actor(&storage,&film,"a").unwrap().len(),1);
    scan(&storage,&root,Some(&source)); drop(storage);
    let storage = Storage::open(&home.path().join("data")).unwrap();
    assert_eq!(catalog::list_films(&storage).unwrap().iter().find(|f|f.id==film).unwrap().actors.len(),1);
    assert!(actors::remove_film_cast_actor(&storage,&film,"b").unwrap().is_empty());
    assert!(actors::remove_film_cast_actor(&storage,&film,"b").unwrap().is_empty());
}

#[test]
fn cast_candidates_are_saved_actors_and_revalidate_deleted_ids_at_write_time() {
    let (home,storage,root,source,film) = fixture();
    assert_eq!(actors::list_cast_candidates(&storage,&film).unwrap().len(),2);
    let second = home.path().join("other"); touch(&second,"film/two.mp4");
    let second_source = scan(&storage,&second,None);
    for state in ["pause","remove"] {
        change_source_state(&storage,&second_source,state).unwrap();
        assert_eq!(actors::list_cast_candidates(&storage,&film).unwrap().len(),2);
        assert_eq!(actors::add_film_cast_actor(&storage,&film,"b").unwrap().len(),2);
    }
    fs::rename(&root,home.path().join("offline")).unwrap();
    assert_eq!(actors::list_cast_candidates(&storage,&film).unwrap().len(),2);
    actors::remove_film_cast_actor(&storage,&film,"b").unwrap();
    storage.with_connection(|db| {db.execute("DELETE FROM actors WHERE id='b'",[])?;Ok(())}).unwrap();
    assert_eq!(actors::list_cast_candidates(&storage,&film).unwrap().len(),1);
    assert_eq!(actors::add_film_cast_actor(&storage,&film,"b").unwrap_err(),"cast_candidate_unavailable");
    assert_eq!(actors::add_film_cast_actor(&storage,&film,"made-up").unwrap_err(),"cast_candidate_unavailable");
    assert_eq!(actors::list_cast_candidates(&storage,"unknown-film").unwrap_err(),"film_not_found");
    change_source_state(&storage,&source,"pause").unwrap();
    assert_eq!(actors::list_cast_candidates(&storage,&film).unwrap_err(),"film_inactive");
    assert_eq!(actors::add_film_cast_actor(&storage,&film,"a").unwrap_err(),"film_inactive");
    assert_eq!(actors::remove_film_cast_actor(&storage,&film,"a").unwrap_err(),"film_inactive");
}

#[test]
fn cast_alias_removal_clears_all_alias_links_for_only_the_selected_actor() {
    let (_home,storage,_root,_source,film) = fixture();
    storage.with_connection(|db| {
        db.execute("INSERT INTO films(id) VALUES('old-alias')",[])?;
        db.execute("INSERT INTO film_aliases(id,film_id) VALUES('old-alias',?1)",[&film])?;
        db.execute("INSERT INTO film_actors(film_id,actor_id,source) VALUES('old-alias','a','manual'),('old-alias','b','manual'),(?1,'b','manual')",[&film])?;
        Ok(())
    }).unwrap();
    assert_eq!(actors::remove_film_cast_actor(&storage,"old-alias","a").unwrap().len(),1);
    storage.with_connection(|db| {
        assert_eq!(db.query_row::<i64,_,_>("SELECT COUNT(*) FROM film_actors WHERE actor_id='a'",[],|r|r.get(0))?,0);
        assert_eq!(db.query_row::<i64,_,_>("SELECT COUNT(*) FROM film_actors WHERE actor_id='b'",[],|r|r.get(0))?,1);
        assert_eq!(db.query_row::<String,_,_>("SELECT film_id FROM film_actors WHERE actor_id='b'",[],|r|r.get(0))?,film);
        Ok(())
    }).unwrap();
    assert!(actors::remove_film_cast_actor(&storage,"old-alias","b").unwrap().is_empty());
}

#[test]
fn failed_cast_writes_preserve_entire_cast_and_other_manual_data() {
    let (_home,storage,_root,_source,film) = fixture();
    catalog::set_favorite(&storage,&film,true).unwrap();
    storage.with_connection(|db|db.execute_batch("CREATE TRIGGER reject_cast_add BEFORE INSERT ON film_actors WHEN NEW.source='manual' BEGIN SELECT RAISE(ABORT,'synthetic'); END;")).unwrap();
    assert_eq!(actors::add_film_cast_actor(&storage,&film,"b").unwrap_err(),"cast_save_failed");
    let films = catalog::list_films(&storage).unwrap(); let current = films.iter().find(|f|f.id==film).unwrap();
    assert!(current.favorite); assert_eq!(current.actors.len(),1);
    storage.with_connection(|db|db.execute_batch("DROP TRIGGER reject_cast_add;")).unwrap();
    actors::add_film_cast_actor(&storage,&film,"b").unwrap();
    storage.with_connection(|db|db.execute_batch("CREATE TRIGGER reject_cast_remove BEFORE DELETE ON film_actors WHEN OLD.source='manual' BEGIN SELECT RAISE(ABORT,'synthetic'); END;")).unwrap();
    assert_eq!(actors::remove_film_cast_actor(&storage,&film,"b").unwrap_err(),"cast_save_failed");
    let films = catalog::list_films(&storage).unwrap(); let current = films.iter().find(|f|f.id==film).unwrap();
    assert!(current.favorite); assert_eq!(current.actors.len(),2);
}

#[test]
fn backup_restore_recovers_manual_cast_without_schema_change() {
    let (_home,storage,_root,_source,film) = fixture();
    actors::add_film_cast_actor(&storage,&film,"b").unwrap();
    let schema = storage.schema_version().unwrap(); let backup = storage.create_backup().unwrap();
    actors::remove_film_cast_actor(&storage,&film,"b").unwrap(); storage.restore_backup(&backup).unwrap();
    assert_eq!(storage.schema_version().unwrap(),schema);
    let films = catalog::list_films(&storage).unwrap(); let cast = &films.iter().find(|f|f.id==film).unwrap().actors;
    assert_eq!(cast.len(),2); assert!(cast.iter().all(|actor|!actor.system));
    assert_eq!(actors::remove_film_cast_actor(&storage,&film,"a").unwrap().len(),1);
    assert!(actors::remove_film_cast_actor(&storage,&film,"b").unwrap().is_empty());
}
