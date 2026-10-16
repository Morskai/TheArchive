use local_film_library_preview_lib::{problems, source_scan::SourceScanSession, storage::Storage};
use std::{fs,path::Path};
fn scan(s:&Storage,root:&Path,id:Option<&str>)->String{let session=SourceScanSession::new();let p=session.preview(s,root,id).unwrap();session.confirm(s,&p.token.unwrap()).unwrap().library_id}
fn setup()->(tempfile::TempDir,Storage,String){let home=tempfile::tempdir().unwrap();let root=home.path().join("synthetic-source");for name in ["[ONE] first","[TWO] second"] {fs::create_dir_all(root.join(name)).unwrap();fs::write(root.join(name).join("main.mp4"),[]).unwrap();}let s=Storage::open(&home.path().join("software")).unwrap();let id=scan(&s,&root,None);(home,s,id)}
#[test]
fn bulk_restore_retains_identity_and_pending_state_after_reopen_and_rescan(){
    let(home,s,id)=setup();let rows=problems::list(&s).unwrap();assert_eq!(rows.len(),2);
    let ids:Vec<_>=rows.iter().map(|p|p.id.clone()).collect();problems::mark(&s,&ids).unwrap();assert!(problems::list(&s).unwrap().iter().all(|p|p.handled));
    problems::restore(&s,&ids).unwrap();drop(s);let s=Storage::open(&home.path().join("software")).unwrap();scan(&s,&home.path().join("synthetic-source"),Some(&id));
    let rows=problems::list(&s).unwrap();assert_eq!(rows.len(),2);assert!(rows.iter().all(|p|!p.handled&&ids.contains(&p.id)));
}
#[test]
fn cleared_records_stay_hidden_on_read_and_reopen_then_complete_scan_recreates_them(){
    let(home,s,id)=setup();let ids:Vec<_>=problems::list(&s).unwrap().iter().map(|p|p.id.clone()).collect();
    problems::mark(&s,&[ids[0].clone()]).unwrap();problems::clear(&s,&ids).unwrap();assert!(problems::list(&s).unwrap().is_empty());
    drop(s);let s=Storage::open(&home.path().join("software")).unwrap();assert!(problems::list(&s).unwrap().is_empty());
    scan(&s,&home.path().join("synthetic-source"),Some(&id));let rows=problems::list(&s).unwrap();assert_eq!(rows.len(),2);assert!(rows.iter().all(|p|!p.handled));
    problems::mark(&s,&[rows[0].id.clone()]).unwrap();scan(&s,&home.path().join("synthetic-source"),Some(&id));let again=problems::list(&s).unwrap();assert_eq!(again.len(),2);assert_eq!(again.iter().filter(|p|p.handled).count(),1);
}
#[test]
fn clear_does_not_recreate_unrelated_source_until_that_source_scans(){
    let(home,s,id)=setup();let other=home.path().join("synthetic-other");for name in ["[THREE] third","[FOUR] fourth"] {fs::create_dir_all(other.join(name)).unwrap();fs::write(other.join(name).join("main.mp4"),[]).unwrap();}let second=scan(&s,&other,None);
    let ids:Vec<_>=problems::list(&s).unwrap().iter().filter(|p|p.sources.iter().any(|source|source.id==id)).map(|p|p.id.clone()).collect();problems::clear(&s,&ids).unwrap();
    scan(&s,&other,Some(&second));assert_eq!(problems::list(&s).unwrap().len(),2);
    scan(&s,&home.path().join("synthetic-source"),Some(&id));assert_eq!(problems::list(&s).unwrap().len(),4);
}
#[test]
fn restore_and_clear_write_failures_roll_back_entire_batch(){
    for action in ["restore","clear"]{let(_home,s,_)=setup();let ids:Vec<_>=problems::list(&s).unwrap().iter().map(|p|p.id.clone()).collect();problems::mark(&s,&ids).unwrap();
    s.with_connection(|db|{let sql=if action=="restore" {format!("CREATE TRIGGER reject_action BEFORE UPDATE ON issue_records WHEN OLD.id='{}' AND NEW.handled_at IS NULL BEGIN SELECT RAISE(ABORT,'synthetic'); END",ids[1])}else{format!("CREATE TRIGGER reject_action BEFORE DELETE ON issue_records WHEN OLD.id='{}' BEGIN SELECT RAISE(ABORT,'synthetic'); END",ids[1])};db.execute_batch(&sql)}).unwrap();
    assert!(if action=="restore"{problems::restore(&s,&ids)}else{problems::clear(&s,&ids)}.is_err());let rows=problems::list(&s).unwrap();assert_eq!(rows.len(),2);assert!(rows.iter().all(|p|p.handled));
    s.with_connection(|db|{assert_eq!(db.query_row("SELECT COUNT(*) FROM issue_records WHERE subject_type='library_problem_cleared'",[],|r|r.get::<_,i64>(0))?,0);Ok(())}).unwrap();}
}
#[test]
fn failed_scan_rolls_back_clear_suppression_and_problem_state(){
    let(home,s,id)=setup();let ids:Vec<_>=problems::list(&s).unwrap().iter().map(|p|p.id.clone()).collect();problems::clear(&s,&ids).unwrap();
    s.with_connection(|db|db.execute_batch("CREATE TRIGGER reject_scan BEFORE UPDATE ON issue_records WHEN NEW.subject_type='cover_scan' BEGIN SELECT RAISE(ABORT,'synthetic'); END")).unwrap();
    let session=SourceScanSession::new();let p=session.preview(&s,&home.path().join("synthetic-source"),Some(&id)).unwrap();assert!(session.confirm(&s,&p.token.unwrap()).is_err());assert!(problems::list(&s).unwrap().is_empty());
}
