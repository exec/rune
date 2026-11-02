use rune_core::{
    editor::Editor,
    file_io::{save, SaveError, SaveOptions},
};

#[test]
fn headless_save_preserves_text_metadata_and_conflict_protection() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested/file.txt");
    let mut editor = Editor::new_buffer();
    editor.insert_text("café\r\n界\n");
    let outcome = save(&mut editor, &path, SaveOptions::default()).unwrap();
    assert_eq!(outcome.created_parent.as_deref(), path.parent());
    assert!(outcome.backup_error.is_none());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "café\r\n界\n");
    assert_eq!(editor.file_path.as_deref(), Some(path.as_path()));
    assert!(!editor.modified);
    editor.insert_text("unsaved");
    std::fs::write(&path, "external").unwrap();
    assert!(matches!(
        save(
            &mut editor,
            &path,
            SaveOptions {
                overwrite: true,
                ..Default::default()
            }
        ),
        Err(SaveError::ChangedOnDisk)
    ));
    assert!(editor.modified);
    assert_eq!(std::fs::read_to_string(path).unwrap(), "external");
}

#[test]
fn overwrite_requires_explicit_confirmation_and_backup_keeps_old_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("file");
    std::fs::write(&path, "old").unwrap();
    let mut editor = Editor::new_buffer();
    editor.insert_text("new");
    assert!(matches!(
        save(&mut editor, &path, SaveOptions::default()),
        Err(SaveError::DestinationExists)
    ));
    assert!(editor.file_path.is_none());
    save(
        &mut editor,
        &path,
        SaveOptions {
            backup: true,
            overwrite: true,
        },
    )
    .unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("file~")).unwrap(),
        "old"
    );
}

#[test]
fn backup_failure_is_reported_separately_from_successful_save() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("file");
    std::fs::write(&path, "old").unwrap();
    std::fs::create_dir(dir.path().join("file~")).unwrap();
    let mut editor = Editor::new_buffer();
    editor.load_file(path.clone()).unwrap();
    editor.insert_text("new ");
    let outcome = save(
        &mut editor,
        &path,
        SaveOptions {
            backup: true,
            overwrite: false,
        },
    )
    .unwrap();
    assert!(outcome.backup_error.is_some());
    assert_eq!(std::fs::read_to_string(path).unwrap(), "new old");
    assert!(!editor.modified);
}
