//! Directory grouping, with old identities retained as aliases. Never touches media.
use rusqlite::{Connection, Transaction, params};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

#[derive(Clone)]
pub(crate) struct VideoRow {
    pub id: String,
    pub film: Option<String>,
    pub source: String,
    pub path: String,
    pub root_name: String,
    pub(crate) confirmed_film: bool,
    created: String,
}

pub(crate) fn rows(db: &Connection) -> rusqlite::Result<Vec<VideoRow>> {
    rows_with_scope(db, true)
}

// Identity anchors survive full refreshes; these rows never enter current lists.
pub(crate) fn historical_rows(db: &Connection) -> rusqlite::Result<Vec<VideoRow>> {
    rows_with_scope(db, false)
}

fn rows_with_scope(db: &Connection, current_only: bool) -> rusqlite::Result<Vec<VideoRow>> {
    let mut q=db.prepare("SELECT o.id,o.film_id,o.library_id,o.observed_path,l.root_path,COALESCE(f.created_at,'') FROM file_observations o JOIN libraries l ON l.id=o.library_id LEFT JOIN films f ON f.id=o.film_id WHERE o.kind='video' AND l.enabled=1 AND l.removed=0 ORDER BY f.created_at,o.film_id,o.id")?;
    let snapshot = crate::artwork::ScanSnapshot::read(db)?;
    let details = crate::artwork::scan_details(db)?;
    let film_directories: BTreeMap<&str, BTreeSet<&str>> = details.iter()
        .map(|(source, details)| (source.as_str(), details.roles.iter()
            .filter(|role| role.role == crate::directory_roles::Role::Film)
            .map(|role| role.directory.as_str())
            .collect()))
        .collect();
    let rows = q
        .query_map([], |r| {
            let root: String = r.get::<_, Option<String>>(4)?.unwrap_or_default();
            let source: String = r.get(2)?;
            let path: String = r.get(3)?;
            let confirmed_film = film_directories.get(source.as_str())
                .is_some_and(|directories| directories.contains(parent(&path).as_str()));
            Ok(VideoRow {
                id: r.get(0)?,
                film: r.get(1)?,
                source,
                path,
                confirmed_film,
                root_name: Path::new(&root)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into(),
                created: r.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows
        .into_iter()
        .filter(|v| !current_only || snapshot.contains(&v.source, &v.id))
        .collect())
}

/// Direct peers for a single artwork lookup, sharing its source evidence.
pub(crate) fn rows_for_directories(
    db: &Connection,
    directories: &BTreeSet<(String, String)>,
    snapshot: &crate::artwork::ScanSnapshot,
    details: &BTreeMap<String, crate::artwork::ScanDetails>,
) -> rusqlite::Result<Vec<VideoRow>> {
    let mut q = db.prepare("SELECT o.id,o.film_id,o.library_id,o.observed_path,l.root_path,COALESCE(f.created_at,'') FROM file_observations o JOIN libraries l ON l.id=o.library_id LEFT JOIN films f ON f.id=o.film_id WHERE o.kind='video' AND l.enabled=1 AND l.removed=0 AND o.library_id=?1 AND substr(replace(o.observed_path,char(92),'/'),1,length(?2))=?2 AND instr(substr(replace(o.observed_path,char(92),'/'),length(?2)+1),'/')=0 ORDER BY f.created_at,o.film_id,o.id")?;
    let mut result = Vec::new();
    for (source, directory) in directories {
        let prefix = if directory.is_empty() { String::new() } else { format!("{directory}/") };
        let confirmed = details.get(source).is_some_and(|d| d.roles.iter().any(|role| role.role == crate::directory_roles::Role::Film && role.directory == *directory));
        for row in q.query_map([source, &prefix], |r| {
            let root: String = r.get::<_, Option<String>>(4)?.unwrap_or_default();
            Ok(VideoRow { id:r.get(0)?, film:r.get(1)?, source:r.get(2)?, path:r.get(3)?, root_name:Path::new(&root).file_name().unwrap_or_default().to_string_lossy().into_owned(), confirmed_film:confirmed, created:r.get(5)? })
        })? {
            let video = row?;
            if snapshot.contains(&video.source, &video.id) { result.push(video); }
        }
    }
    Ok(result)
}

pub(crate) fn parent(path: &str) -> String {
    path.replace('\\', "/")
        .rsplit_once('/')
        .map(|(dir, _)| dir.to_owned())
        .unwrap_or_default()
}

pub(crate) fn category(name: &str) -> bool {
    let name =
        name.trim_start_matches(|c: char| c.is_ascii_digit() || c == '-' || c == '_' || c == ' ');
    [
        "日本",
        "欧美",
        "亚洲",
        "多人",
        "多人影片",
        "单体",
        "FC2",
        "素人系列",
        "无名",
        "分类",
        "Tokyo",
    ]
    .iter()
    .any(|value| name.eq_ignore_ascii_case(value))
}

pub(crate) fn has_code(name: &str) -> bool {
    name.split('[').skip(1)
        .filter_map(|s| s.split_once(']').map(|(token, _)| token.trim()))
        .any(|token| !token.is_empty() && !token.contains('['))
}

pub(crate) fn key(video:&VideoRow,_all:&[VideoRow])->Option<(String,String)>{Some((video.source.clone(),parent(&video.path)))}

pub(crate) fn plan(db: &Connection) -> rusqlite::Result<Vec<(String, Vec<String>)>> {
    let all = rows(db)?;
    let mut dirs: BTreeMap<(String, String), BTreeMap<String, String>> = BTreeMap::new();
    let mut locations: BTreeMap<String, BTreeSet<(String, String)>> = BTreeMap::new();
    for row in &all {
        if let Some(film) = &row.film {
            locations
                .entry(film.clone())
                .or_default()
                .insert((row.source.clone(), parent(&row.path)));
            if film.starts_with("auto-") {
                if let Some(key) = key(row, &all) {
                    dirs.entry(key)
                        .or_default()
                        .insert(film.clone(), row.created.clone());
                }
            }
        }
    }
    Ok(dirs
        .into_values()
        .filter(|films| films.len() > 1 && films.keys().all(|id| locations[id].len() == 1))
        .map(|films| {
            let primary = films
                .iter()
                .min_by_key(|(id, created)| (*created, *id))
                .unwrap()
                .0
                .clone();
            let members = films.into_keys().filter(|id| id != &primary).collect();
            (primary, members)
        })
        .collect())
}

pub(crate) fn merge(tx: &Transaction<'_>) -> rusqlite::Result<usize> {
    let plan = plan(tx)?;
    let count = plan.iter().map(|(_, members)| members.len()).sum();
    for (primary, members) in plan {
        // The prior development reset was a one-time authorization, never a scan rule.
        // Keep manual rows (including old alias rows) so regrouping cannot destroy them.
        for member in members {
            tx.execute(
                "UPDATE film_aliases SET film_id=?1 WHERE film_id=?2",
                params![primary, member],
            )?;
            tx.execute("INSERT INTO film_aliases(id,film_id) VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET film_id=excluded.film_id",params![member,primary])?;
            tx.execute(
                "UPDATE file_observations SET film_id=?1 WHERE film_id=?2",
                params![primary, member],
            )?;
        }
    }
    Ok(count)
}

pub(crate) fn clean_title(value: &str) -> String {
    let mut result = String::new();
    let mut remaining = value;
    while let Some(c) = remaining.chars().next() {
        if let Some(end) = match c {
            '[' => Some(']'),
            '【' => Some('】'),
            _ => None,
        } {
            if let Some(index) = remaining.find(end) {
                remaining = &remaining[index + end.len_utf8()..];
                continue;
            }
        }
        result.push(c);
        remaining = &remaining[c.len_utf8()..];
    }
    result.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(crate) fn directory_code(name:&str)->Option<String> {
 name.split('[').skip(1).filter_map(|s|s.split_once(']').map(|(token,_)|token.trim())).find(|s|!s.is_empty()&&!s.contains('[')).map(str::to_owned)
}

pub(crate) fn title(path: &str, root_name: &str, confirmed_film: bool) -> String {
    let dir = parent(path);
    let folder = if dir.is_empty() {
        root_name
    } else {
        dir.rsplit('/').next().unwrap_or_default()
    };
    if confirmed_film || (!category(folder) && (!dir.is_empty() || has_code(folder))) {
        let title = clean_title(folder);
        if !title.is_empty() {
            return title;
        }
    }
    let file = Path::new(path)
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy();
    let title = clean_title(&file);
    if title.is_empty()
        || has_code(folder) && title.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        "未提供片名".into()
    } else {
        title
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn folder_title_is_separate_from_code_and_preserves_unclosed_text() {
        assert_eq!(
            title("[DEMO-01] 虚构片名【标签】/DEMO-01.mp4", "分类", false),
            "虚构片名"
        );
        assert_eq!(
            title("DEMO-01.mp4", "[DEMO-01] 入口片名【标签】", false),
            "入口片名"
        );
        assert_eq!(title("虚构视频.mp4", "0-单体", false), "虚构视频");
        assert_eq!(clean_title("[DEMO-01] 片名【未闭合"), "片名【未闭合");
    }
}
