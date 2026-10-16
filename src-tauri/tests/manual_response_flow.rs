use local_film_library_preview_lib::{actor_management::{self,SaveActorInput},actors,catalog,playback,source_scan::SourceScanSession,storage::Storage};
use std::fs;
#[test]
fn rapid_manual_values_preserve_latest_choice_reopen_and_failed_updates_roll_back(){
    let home=tempfile::tempdir().unwrap();let root=home.path().join("synthetic-source");fs::create_dir_all(root.join("actor/film")).unwrap();
    image::RgbImage::new(8,8).save(root.join("actor/actor.jpg")).unwrap();fs::write(root.join("actor/film/one.mp4"),[]).unwrap();fs::write(root.join("actor/film/two.mkv"),[]).unwrap();
    let s=Storage::open(&home.path().join("data")).unwrap();let session=SourceScanSession::new();let p=session.preview(&s,&root,None).unwrap();session.confirm(&s,&p.token.unwrap()).unwrap();
    let film=catalog::list_films(&s).unwrap().remove(0);
    actor_management::save(&s,&SaveActorInput {actor_id:None,name:"人工演员".into(),aliases:vec![],profile:actors::Profile::default(),photo_path:Some(root.join("actor/actor.jpg").to_string_lossy().into_owned()),remove_photo:false,film_ids:vec![film.id.clone()]}).unwrap();
    let actor=actors::list_actors(&s).unwrap().remove(0);let second=&film.videos[1].id;
    let photo=actors::thumbnail(&s,&actor.id,false).unwrap();
    catalog::set_rating(&s,&film.id,Some(3)).unwrap();catalog::set_rating(&s,&film.id,Some(5)).unwrap();
    playback::set_preferred(&s,&film.id,second,true).unwrap();playback::set_preferred(&s,&film.id,second,false).unwrap();
    actors::set_favorite(&s,&actor.id,true).unwrap();actors::set_favorite(&s,&actor.id,false).unwrap();
    drop(s);let s=Storage::open(&home.path().join("data")).unwrap();let saved=catalog::list_films(&s).unwrap().remove(0);
    assert_eq!(saved.rating,Some(5.0));assert_eq!(saved.preferred_video_id,None);assert!(!actors::list_actors(&s).unwrap()[0].favorite);
    playback::set_preferred(&s,&film.id,second,true).unwrap();actors::set_favorite(&s,&actor.id,true).unwrap();
    s.with_connection(|db|db.execute_batch("CREATE TRIGGER reject_rating BEFORE UPDATE ON film_manual BEGIN SELECT RAISE(ABORT,'synthetic'); END; CREATE TRIGGER reject_actor BEFORE UPDATE ON actor_manual BEGIN SELECT RAISE(ABORT,'synthetic'); END; CREATE TRIGGER reject_preferred BEFORE INSERT ON file_versions BEGIN SELECT RAISE(ABORT,'synthetic'); END;")).unwrap();
    assert!(catalog::set_rating(&s,&film.id,Some(1)).is_err());assert!(actors::set_favorite(&s,&actor.id,false).is_err());assert!(playback::set_preferred(&s,&film.id,&film.videos[0].id,true).is_err());
    let saved=catalog::list_films(&s).unwrap().remove(0);assert_eq!(saved.rating,Some(5.0));assert_eq!(saved.preferred_video_id.as_ref(),Some(second));assert!(actors::list_actors(&s).unwrap()[0].favorite);
    assert!(actors::thumbnail(&s,&actor.id,false).unwrap()==photo);
}
