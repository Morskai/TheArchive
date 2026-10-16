use local_film_library_preview_lib::actor_info;
use serde_json::json;

#[test]
fn actor_document_keeps_omitted_and_explicit_empty_fields_distinct() {
    let absent = actor_info::parse(&json!({"format_version":1,"kind":"actor","name":"Mira"}).to_string()).unwrap();
    assert!(!absent.contains_key("aliases"));
    let cleared = actor_info::parse(&json!({"format_version":1,"kind":"actor","name":"Mira","aliases":[],"cup":null}).to_string()).unwrap();
    assert_eq!(cleared["aliases"], json!([]));
    assert_eq!(cleared["cup"], json!(null));
}

#[test]
fn archival_fields_never_block_personal_data_parsing() {
    let parsed = actor_info::parse(&json!({"format_version":1,"kind":"actor","name":"Mira","id":{},"photo":"C:/archival-only.jpg","film_ids":42}).to_string()).unwrap();
    assert_eq!(parsed["name"], "Mira");
}

#[test]
fn film_documents_and_unknown_personal_fields_are_rejected() {
    for document in [
        json!({"format_version":1,"kind":"film","name":"Mira"}),
        json!({"format_version":2,"kind":"actor","name":"Mira"}),
        json!({"format_version":1,"kind":"actor","name":"Mira","favorite":true}),
    ] { assert_eq!(actor_info::parse(&document.to_string()).unwrap_err(), "info_invalid"); }
}

#[test]
fn invalid_personal_values_and_control_characters_are_rejected() {
    for field in [json!({"name":"Mi\nra"}), json!({"aliases":[""]}), json!({"height_cm":1.5}), json!({"cup":[]})] {
        let mut document = json!({"format_version":1,"kind":"actor","name":"Mira"});
        document.as_object_mut().unwrap().extend(field.as_object().unwrap().clone());
        assert_eq!(actor_info::parse(&document.to_string()).unwrap_err(), "info_invalid");
    }
}

#[test]
fn malformed_and_oversized_documents_fail_before_import() {
    assert_eq!(actor_info::parse("not JSON").unwrap_err(), "info_invalid");
    assert_eq!(actor_info::parse(&" ".repeat(1024 * 1024 + 1)).unwrap_err(), "info_too_large");
}
