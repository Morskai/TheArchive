//! Library-local image ratios. Appearance saves cannot overwrite these choices.
use crate::{lifecycle, storage::Storage};
use rusqlite::params;
use serde::Serialize;
use std::collections::{BTreeMap,BTreeSet};
type Result<T> = std::result::Result<T,String>;

#[derive(Debug,PartialEq,Eq,Serialize)]
#[serde(rename_all="camelCase")]
pub struct DisplayPreferences {
    pub default_film_ratio: String,
    pub actor_ratio: String,
    pub library_ratios: BTreeMap<String,String>,
}
pub fn load(storage:&Storage)->Result<DisplayPreferences> {
    let prefs=lifecycle::load_preferences(storage)?;
    let library_ratios=storage.with_connection(|db| {
        let mut q=db.prepare("SELECT library_id,cover_ratio FROM library_display_preferences")?;
        q.query_map([],|r|Ok((r.get(0)?,r.get(1)?)))?.collect::<rusqlite::Result<BTreeMap<_,_>>>()
    }).map_err(|_|"preferences_load_failed")?;
    Ok(DisplayPreferences {default_film_ratio:if prefs.film_cover_ratio=="3:2" {"original".into()} else {prefs.film_cover_ratio},actor_ratio:prefs.actor_photo_ratio,library_ratios})
}
pub fn save_film_ratios(storage:&Storage, library_ids:&[String], ratio:&str)->Result<()> {
    if !["original","16:9","5:7"].contains(&ratio) || library_ids.is_empty() {return Err("preferences_save_failed".into())}
    storage.with_transaction(|tx| {
        let ids:BTreeSet<_>=library_ids.iter().collect();
        for id in &ids {tx.query_row("SELECT 1 FROM libraries WHERE id=?1 AND removed=0",[*id],|_|Ok(()))?;}
        for id in ids {tx.execute("INSERT INTO library_display_preferences(library_id,cover_ratio) VALUES(?1,?2) ON CONFLICT(library_id) DO UPDATE SET cover_ratio=excluded.cover_ratio",params![id,ratio])?;}
        Ok(())
    }).map_err(|_|"preferences_save_failed".into())
}
pub fn save_actor_ratio(storage:&Storage, ratio:&str)->Result<()> {
    if !["original","3:4","2:3"].contains(&ratio) {return Err("preferences_save_failed".into())}
    storage.with_transaction(|tx| {
        tx.execute("INSERT OR IGNORE INTO interface_preferences(singleton,reduce_motion,thin_scrollbar) VALUES(1,0,1)",[])?;
        tx.execute("UPDATE interface_preferences SET actor_photo_ratio=?1 WHERE singleton=1",[ratio])?;
        Ok(())
    }).map_err(|_|"preferences_save_failed".into())
}
pub fn save_appearance(storage:&Storage, prefs:&lifecycle::InterfacePreferences)->Result<()> {
    if !["cover","contain"].contains(&prefs.film_cover_fit.as_str()) || !["cover","contain"].contains(&prefs.actor_photo_fit.as_str()) {return Err("preferences_save_failed".into())}
    storage.with_transaction(|tx| {
        tx.execute("INSERT OR IGNORE INTO interface_preferences(singleton,reduce_motion,thin_scrollbar) VALUES(1,0,1)",[])?;
        tx.execute("UPDATE interface_preferences SET reduce_motion=?1,thin_scrollbar=?2,film_cover_fit=?3,actor_photo_fit=?4 WHERE singleton=1",params![prefs.reduce_motion,prefs.thin_scrollbar,prefs.film_cover_fit,prefs.actor_photo_fit])?;
        Ok(())
    }).map_err(|_|"preferences_save_failed".into())
}
