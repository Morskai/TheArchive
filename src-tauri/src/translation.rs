//! Local title translation memory. This module never contacts providers or media.
use crate::{catalog::{self, CatalogFilm}, storage::Storage};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TranslationSettings {
    pub enabled: bool,
    pub provider: String,
    pub deepl_plan: String,
    pub target_language: String,
    #[serde(default)]
    pub azure_region: String,
    #[serde(default)]
    pub baidu_app_id: String,
}
impl Default for TranslationSettings {
    fn default() -> Self {
        Self { enabled:false, provider:"deepl".into(), deepl_plan:"free".into(), target_language:"ZH-HANS".into(), azure_region:String::new(), baidu_app_id:String::new() }
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct TranslationStats {
    pub pending: usize,
    pub automatic: usize,
    pub memory: usize,
    pub local_reuse: usize,
    pub stale: usize,
    pub api_requests: usize,
}
#[derive(Clone, Debug, Serialize)]
pub struct TranslationPreviewEntry {
    pub film_id: String,
    pub title: String,
    pub sources: Vec<TranslationSource>,
    pub original_title: String,
    pub body: String,
    pub status: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct TranslationSource { pub id: String, pub name: String }
#[derive(Clone, Debug, Serialize)]
pub struct TranslationPreview {
    pub summary: TranslationStats,
    pub entries: Vec<TranslationPreviewEntry>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedTranslation {
    pub film_id: String,
    pub original_title: String,
    pub body: String,
    pub fingerprint: String,
    pub normalized_title: String,
    pub normalized_code: String,
    pub identity_anchor: String,
    pub memory_id: Option<String>,
}

pub fn load_settings(storage:&Storage) -> Result<TranslationSettings> {
    storage.with_connection(|db|db.query_row("SELECT enabled,provider,deepl_plan,target_language,azure_region,baidu_app_id FROM translation_settings WHERE singleton=1",[],|r|Ok(TranslationSettings {enabled:r.get(0)?,provider:r.get(1)?,deepl_plan:r.get(2)?,target_language:r.get(3)?,azure_region:r.get(4)?,baidu_app_id:r.get(5)?})).optional())
        .map(|v|v.unwrap_or_default()).map_err(|_|"translation_settings_failed".into())
}
pub fn save_settings(storage:&Storage,settings:&TranslationSettings)->Result<()> {
    if !["deepl","google","azure","baidu"].contains(&settings.provider.as_str()) || !["free","pro"].contains(&settings.deepl_plan.as_str()) || settings.target_language!="ZH-HANS" || settings.azure_region.len()>64 || !settings.azure_region.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b==b'-') || settings.baidu_app_id.len()>64 || !settings.baidu_app_id.bytes().all(|b| b.is_ascii_digit()) {
        return Err("translation_settings_invalid".into());
    }
    storage.with_transaction(|tx| {
        tx.execute("INSERT INTO translation_settings(singleton,enabled,provider,deepl_plan,target_language,azure_region,baidu_app_id) VALUES(1,?1,?2,?3,?4,?5,?6) ON CONFLICT(singleton) DO UPDATE SET enabled=excluded.enabled,provider=excluded.provider,deepl_plan=excluded.deepl_plan,target_language=excluded.target_language,azure_region=excluded.azure_region,baidu_app_id=excluded.baidu_app_id",params![settings.enabled,settings.provider,settings.deepl_plan,settings.target_language,settings.azure_region,settings.baidu_app_id])?;
        Ok(())
    }).map_err(|_|"translation_settings_failed".into())
}

fn normalize_code(value:&str)->String {
    value.chars().filter(|c|c.is_ascii_alphanumeric()).flat_map(char::to_uppercase).collect()
}

// Number is the first nonempty ASCII bracket group; metadata is never title text.
fn code_token(title:&str)->String {
    title.split('[').skip(1).filter_map(|s|s.split_once(']').map(|(token,_)|token.trim()))
        .find(|token|!token.is_empty() && !token.contains('[')).map(normalize_code).unwrap_or_default()
}

fn normalize_title(value:&str)->String {
    let punctuation=[',','，','.','。','!','！','?','？',':','：',';','；','、','"','“','”','\'','‘','’','(',')','（','）'];
    let cleaned:String=value.chars().filter(|c|!punctuation.contains(c)).collect();
    let chars:Vec<char>=cleaned.chars().collect();
    let mut result=String::new();
    for (i,c) in chars.iter().enumerate() {
        if c.is_whitespace() {
            let before=chars[..i].iter().rev().find(|c|!c.is_whitespace());
            let after=chars[i+1..].iter().find(|c|!c.is_whitespace());
            if before.is_some_and(|c|c.is_ascii_alphanumeric()) && after.is_some_and(|c|c.is_ascii_alphanumeric()) && !result.ends_with(' ') { result.push(' '); }
        } else {result.push(*c);}
    }
    result.trim().to_owned()
}

fn evidence(film:&CatalogFilm)->PreparedTranslation {
    let code=code_token(&film.original_title);
    let body=crate::folder_groups::clean_title(&film.original_title);
    let normalized_title=normalize_title(&body);
    let fingerprint=format!("{:x}",Sha256::digest(normalized_title.as_bytes()));
    PreparedTranslation {film_id:film.id.clone(),identity_anchor:film.id.clone(),original_title:film.original_title.clone(),body,normalized_title,fingerprint,normalized_code:code,memory_id:None}
}

fn needs_translation(body:&str)->bool {
    body.chars().any(char::is_alphabetic)
}

fn language_uncertain(body:&str)->bool {
    let has_han=body.chars().any(|c|matches!(c as u32,0x3400..=0x9fff|0xf900..=0xfaff));
    let has_other_letters=body.chars().any(|c|c.is_alphabetic() && !matches!(c as u32,0x3400..=0x9fff|0xf900..=0xfaff));
    has_han && !has_other_letters
}

#[derive(Clone)]
struct Memory {
    id:String,
    normalized_code:String,
    normalized_title:String,
    fingerprint:String,
    translated_title:String,
    provider:String,
}

fn memory_row(r:&rusqlite::Row<'_>)->rusqlite::Result<Memory> {
    Ok(Memory {id:r.get(0)?,normalized_code:r.get(1)?,normalized_title:r.get(2)?,fingerprint:r.get(3)?,translated_title:r.get(4)?,provider:r.get(5)?})
}
fn exact_match(memory:&Memory,current:&PreparedTranslation)->bool {
    memory.normalized_code==current.normalized_code && memory.normalized_title==current.normalized_title && memory.fingerprint==current.fingerprint
}
fn linked_memory(db:&Connection,id:&str)->rusqlite::Result<Option<Memory>> {
    db.query_row("SELECT m.id,m.normalized_code,m.normalized_title,m.fingerprint,m.translated_title,m.provider FROM film_translations f JOIN translation_memory m ON m.id=f.memory_id WHERE f.film_id=?1",[id],memory_row).optional()
}
fn strong_memory(db:&Connection,current:&PreparedTranslation)->rusqlite::Result<Option<Memory>> {
    let mut q=db.prepare("SELECT m.id,m.normalized_code,m.normalized_title,m.fingerprint,m.translated_title,m.provider FROM translation_memory m LEFT JOIN film_aliases a ON a.id=m.identity_anchor WHERE m.normalized_code=?1 AND m.normalized_title=?2 AND m.fingerprint=?3 AND (?1<>'' OR COALESCE(a.film_id,m.identity_anchor)=?4) AND (m.provider NOT IN ('user','original') OR COALESCE(a.film_id,m.identity_anchor)=?4) ORDER BY m.translated_at DESC,m.rowid DESC")?;
    let rows:Vec<Memory>=q.query_map(params![current.normalized_code,current.normalized_title,current.fingerprint,current.identity_anchor],memory_row)?.collect::<rusqlite::Result<_>>()?;
    // Conflicting historical results are not silently selected. A linked record
    // remains authoritative for its film; a new film needs an unambiguous result.
    if let Some(memory)=rows.iter().find(|m| matches!(m.provider.as_str(),"user"|"original")) {return Ok(Some(memory.clone()));}
    let values:BTreeSet<_>=rows.iter().map(|m|m.translated_title.as_str()).collect();
    Ok(if values.len()==1 {rows.into_iter().next()} else {None})
}

pub(crate) fn present_films(db:&Connection,films:&mut [CatalogFilm])->rusqlite::Result<()> {
    for film in films {
        let current=evidence(film);
        let linked=linked_memory(db,&film.id)?;
        let memory=match linked {
            Some(memory) if matches!(memory.provider.as_str(),"user"|"original") || exact_match(&memory,&current)=>Some(memory),
            previous=>strong_memory(db,&current)?.or(previous),
        };
        if let Some(memory)=memory {
            if memory.provider=="original" {
                // Reattach the choice before returning so later scans cannot revive an API title.
                db.execute("INSERT INTO film_translations(film_id,memory_id) VALUES(?1,?2) ON CONFLICT(film_id) DO UPDATE SET memory_id=excluded.memory_id",params![film.id,memory.id])?;
                continue;
            }
            film.title_edited=memory.provider=="user";
            film.translation_stale=!exact_match(&memory,&current);
            film.automatic_title=Some(memory.translated_title.clone());
            if !film.translation_stale {
                db.execute("INSERT INTO film_translations(film_id,memory_id) VALUES(?1,?2) ON CONFLICT(film_id) DO UPDATE SET memory_id=excluded.memory_id",params![film.id,memory.id])?;
                db.execute("UPDATE translation_memory SET last_matched_at=CURRENT_TIMESTAMP WHERE id=?1",[&memory.id])?;
            }
            film.title=if film.title_edited {memory.translated_title} else {crate::folder_groups::clean_title(&memory.translated_title)};
        }
    }
    Ok(())
}

pub fn preview(storage:&Storage)->Result<TranslationPreview> {
    let films=catalog::list_films(storage).map_err(|_|"translation_load_failed")?;
    let memory=storage.with_connection(|db|db.query_row("SELECT count(*) FROM translation_memory",[],|r|r.get::<_,usize>(0))).map_err(|_|"translation_load_failed")?;
    let mut summary=TranslationStats {memory,..Default::default()};
    let mut bodies=BTreeSet::new();
    let entries=films.into_iter().map(|film| {
        let current=evidence(&film);
        if film.automatic_title.is_some() {summary.automatic+=1;}
        if film.translation_stale {summary.stale+=1;}
        let status=if film.title_edited {"manual"}
            else if film.translation_stale {"stale"}
            else if film.automatic_title.is_some() {summary.local_reuse+=1;"automatic"}
            else if !needs_translation(&current.body) {"chinese"}
            else {summary.pending+=1;bodies.insert(current.body.clone());if language_uncertain(&current.body) {"language_uncertain"} else {"pending"}};
        let sources = film.videos.iter().map(|video|(video.source_id.clone(),video.source_name.clone())).collect::<std::collections::BTreeMap<_,_>>()
            .into_iter().map(|(id,name)|TranslationSource{id,name}).collect();
        TranslationPreviewEntry {film_id:film.id,title:film.title,sources,original_title:film.original_title,body:current.body,status:status.into()}
    }).collect();
    summary.api_requests=bodies.len();
    Ok(TranslationPreview {summary,entries})
}
pub fn stats(storage:&Storage)->Result<TranslationStats> {Ok(preview(storage)?.summary)}

pub fn prepare_translation(storage:&Storage,film_id:&str,force:bool)->Result<Option<PreparedTranslation>> {
    let (film,memory_id)=storage.with_transaction(|tx| {
        let mut films=catalog::raw_films_for(tx,true,Some(film_id))?;
        present_films(tx,&mut films)?;
        Ok((films.into_iter().next(),linked_memory(tx,film_id)?.map(|m|m.id)))
    }).map_err(|_|"translation_load_failed")?;
    let film=film.ok_or("translation_film_missing")?;
    if film.title_edited || (!force && film.automatic_title.is_some()) {return Ok(None);}
    let mut current=evidence(&film);
    current.memory_id=memory_id;
    if !needs_translation(&current.body) {return Ok(None);}
    Ok(Some(current))
}

pub fn save_translation(storage:&Storage,prepared:&PreparedTranslation,provider:&str,text:&str,source_language:&str)->Result<()> {
    let text=text.trim();
    if text.is_empty() || text.chars().count()>20000 || !["deepl","google","azure","baidu"].contains(&provider) || source_language.len()>32 {return Err("translation_result_invalid".into());}
    storage.with_transaction(|tx| {
        let film=catalog::raw_films_for(tx,true,Some(&prepared.film_id))?.into_iter().next().ok_or(rusqlite::Error::QueryReturnedNoRows)?;
        let expected=PreparedTranslation {memory_id:None,..prepared.clone()};
        if evidence(&film)!=expected || linked_memory(tx,&film.id)?.map(|m|m.id)!=prepared.memory_id {return Err(rusqlite::Error::InvalidParameterName("translation_changed".into()));}
        // A deliberate retranslation replaces the authoritative result for this
        // exact evidence, rather than creating contradictory memories. Changed
        // evidence gets its own record so the older original can still return.
        let previous=linked_memory(tx,&prepared.film_id)?.filter(|memory|exact_match(memory,prepared));
        let id=if let Some(memory)=previous {
            tx.execute("UPDATE translation_memory SET original_title=?2,body=?3,source_language=?4,translated_title=?5,provider=?6,translated_at=CURRENT_TIMESTAMP,last_matched_at=CURRENT_TIMESTAMP WHERE id=?1",params![memory.id,prepared.original_title,prepared.body,source_language,text,provider])?;
            memory.id
        } else {
            let id:String=tx.query_row("SELECT lower(hex(randomblob(16)))",[],|r|r.get(0))?;
            tx.execute("INSERT INTO translation_memory(id,identity_anchor,normalized_code,original_title,normalized_title,body,fingerprint,source_language,translated_title,provider) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",params![id,prepared.identity_anchor,prepared.normalized_code,prepared.original_title,prepared.normalized_title,prepared.body,prepared.fingerprint,source_language,text,provider])?;
            id
        };
        tx.execute("INSERT INTO film_translations(film_id,memory_id) VALUES(?1,?2) ON CONFLICT(film_id) DO UPDATE SET memory_id=excluded.memory_id",params![prepared.film_id,id])?;
        Ok(())
    }).map_err(|error| if matches!(error.downcast_ref::<rusqlite::Error>(),Some(rusqlite::Error::InvalidParameterName(value)) if value=="translation_changed") || matches!(error.downcast_ref::<rusqlite::Error>(),Some(rusqlite::Error::QueryReturnedNoRows)) {"translation_changed".into()} else {"translation_save_failed".into()})
}

// Edited titles are film-scoped memory: never mutate a shared automatic record.
pub fn edit_title(storage:&Storage,film_id:&str,expected_original:&str,expected_title:&str,text:&str)->Result<CatalogFilm> {
    let text=text.trim();
    if text.chars().count()>20000 || text.contains('\0') || text.contains(['\r','\n']) {return Err("translation_result_invalid".into());}
    storage.with_transaction(|tx| {
        let mut films=catalog::raw_films_for(tx,true,Some(film_id))?;
        present_films(tx,&mut films)?;
        let film=films.into_iter().next().ok_or(rusqlite::Error::QueryReturnedNoRows)?;
        if film.original_title!=expected_original || film.title!=expected_title {return Err(rusqlite::Error::InvalidParameterName("translation_changed".into()));}
        edit_title_tx(tx,film_id,text)?;
        let mut films=catalog::raw_films_for(tx,true,Some(film_id))?;
        present_films(tx,&mut films)?;
        films.into_iter().next().ok_or(rusqlite::Error::QueryReturnedNoRows)
    }).map_err(|error|if matches!(error.downcast_ref::<rusqlite::Error>(),Some(rusqlite::Error::InvalidParameterName(value)) if value=="translation_changed") || matches!(error.downcast_ref::<rusqlite::Error>(),Some(rusqlite::Error::QueryReturnedNoRows)) {"translation_changed".into()} else {"translation_save_failed".into()})
}

pub(crate) fn edit_title_tx(tx:&rusqlite::Transaction<'_>,film_id:&str,text:&str)->rusqlite::Result<()> {
 let mut films=catalog::raw_films_for(tx,true,Some(film_id))?;present_films(tx,&mut films)?;let film=films.into_iter().next().ok_or(rusqlite::Error::QueryReturnedNoRows)?;
         let current=evidence(&film);
        let id:String=tx.query_row("SELECT lower(hex(randomblob(16)))",[],|r|r.get(0))?;
        let value=if text.is_empty() {crate::folder_groups::title(&film.videos[0].observed_path,&film.original_title,true)} else {text.to_owned()};
        let provider=if text.is_empty() {"original"} else {"user"};
        tx.execute("INSERT INTO translation_memory(id,identity_anchor,normalized_code,original_title,normalized_title,body,fingerprint,translated_title,provider) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",params![id,current.identity_anchor,current.normalized_code,current.original_title,current.normalized_title,current.body,current.fingerprint,value,provider])?;
        tx.execute("INSERT INTO film_translations(film_id,memory_id) VALUES(?1,?2) ON CONFLICT(film_id) DO UPDATE SET memory_id=excluded.memory_id",params![film_id,id])?;

 Ok(())
}

pub fn clear_memory(storage:&Storage)->Result<usize> {
    clear_memory_checked(storage,None)
}

pub fn clear_memory_expected(storage:&Storage,expected_count:usize)->Result<usize> {
    clear_memory_checked(storage,Some(expected_count))
}

fn clear_memory_checked(storage:&Storage,expected_count:Option<usize>)->Result<usize> {
    storage.create_protection().map_err(|_|"protection_failed")?;
    storage.with_transaction(|tx| {
        let count=tx.query_row("SELECT count(*) FROM translation_memory",[],|r|r.get::<_,usize>(0))?;
        if expected_count.is_some_and(|expected|expected!=count) {
            return Err(rusqlite::Error::InvalidParameterName("translation_memory_changed".into()));
        }
        tx.execute("DELETE FROM film_translations",[])?;
        tx.execute("DELETE FROM translation_memory",[])?;
        Ok(count)
    }).map_err(|error|if matches!(error.downcast_ref::<rusqlite::Error>(),Some(rusqlite::Error::InvalidParameterName(value)) if value=="translation_memory_changed") {"translation_memory_changed".into()} else {"translation_clear_failed".into()})
}
