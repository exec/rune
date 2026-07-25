//! Regression guards for the security findings of the v1.5.3 review:
//! backup-on-save writing through a hostile `<path>~`, and terminal escape
//! sequences from file content reaching the terminal verbatim.

use ratatui::style::Style;
use ratatui::text::Span;
use rune::ui::strip_control_chars;
use std::io::Write;

mod backup {
    use rune::config::Config;
    use rune::tabs::TabManager;
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    fn saving_tab(path: &std::path::Path, contents: &str) -> TabManager {
        let mut tabs = TabManager::new_for_test();
        tabs.config = Config {
            backup_on_save: true,
            ..Default::default()
        };
        tabs.active_editor_mut().rope = ropey::Rope::from_str(contents);
        tabs.active_editor_mut().file_path = Some(path.to_path_buf());
        tabs
    }

    /// A pre-existing backup path owned by someone else must not be written
    /// through. `fs::copy` truncated it in place, preserving their inode, so a
    /// private file's contents landed in a file they could chmod and read.
    #[test]
    fn backup_does_not_write_into_a_preexisting_inode() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("doc.txt");
        std::fs::write(&target, "secret contents\n").unwrap();

        // Stand-in for the attacker's pre-planted file.
        let backup = dir.path().join("doc.txt~");
        std::fs::write(&backup, "planted\n").unwrap();

        #[cfg(unix)]
        let inode_before = {
            use std::os::unix::fs::MetadataExt;
            std::fs::metadata(&backup).unwrap().ino()
        };

        let mut tabs = saving_tab(&target, "new contents\n");
        tabs.perform_save(target.clone()).unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let inode_after = std::fs::metadata(&backup).unwrap().ino();
            assert_ne!(
                inode_before, inode_after,
                "backup reused the pre-existing inode instead of creating a fresh file"
            );
        }
        // The backup must still be correct: the pre-save contents of the target.
        assert_eq!(
            std::fs::read_to_string(&backup).unwrap(),
            "secret contents\n"
        );
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "new contents\n");
    }

    /// A symlink at the backup path must not redirect the copy.
    #[cfg(unix)]
    #[test]
    fn backup_does_not_follow_a_planted_symlink() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("doc.txt");
        std::fs::write(&target, "secret contents\n").unwrap();

        let canary = dir.path().join("canary.txt");
        std::fs::write(&canary, "untouched\n").unwrap();
        std::os::unix::fs::symlink(&canary, dir.path().join("doc.txt~")).unwrap();

        let mut tabs = saving_tab(&target, "new contents\n");
        tabs.perform_save(target.clone()).unwrap();

        assert_eq!(
            std::fs::read_to_string(&canary).unwrap(),
            "untouched\n",
            "backup followed the symlink and clobbered the canary"
        );
    }

    /// A FIFO at the backup path used to block the UI thread forever in
    /// `open(O_WRONLY)` with no timeout: the save never returned.
    #[cfg(unix)]
    #[test]
    fn backup_does_not_hang_on_a_fifo() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("doc.txt");
        std::fs::write(&target, "secret contents\n").unwrap();

        let fifo = dir.path().join("doc.txt~");
        let status = std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .expect("mkfifo should be available");
        assert!(status.success());

        let (tx, rx) = mpsc::channel();
        let target_for_thread = target.clone();
        thread::spawn(move || {
            let mut tabs = saving_tab(&target_for_thread, "new contents\n");
            let r = tabs.perform_save(target_for_thread.clone());
            let _ = tx.send(r.is_ok());
        });

        let finished = rx
            .recv_timeout(Duration::from_secs(10))
            .expect("perform_save hung on a FIFO at the backup path");
        // The save itself must still succeed; only the backup is skipped.
        assert!(
            finished,
            "save failed outright instead of skipping the backup"
        );
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "new contents\n");
    }

    /// The ordinary case must keep working, and on Unix the backup should carry
    /// the source file's mode rather than the umask default.
    #[test]
    fn backup_of_a_plain_file_still_works() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("doc.txt");
        std::fs::write(&target, "old\n").unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600)).unwrap();
        }

        let mut tabs = saving_tab(&target, "new\n");
        tabs.perform_save(target.clone()).unwrap();

        let backup = dir.path().join("doc.txt~");
        assert_eq!(std::fs::read_to_string(&backup).unwrap(), "old\n");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&backup).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "backup did not inherit the source file's mode");
        }
    }
}

mod escape_sequences {
    use super::*;

    /// The core guarantee: nothing that could be interpreted as a terminal
    /// control sequence survives into rendered text.
    #[test]
    fn control_chars_are_stripped() {
        // OSC window-title injection, the classic payload.
        assert_eq!(
            strip_control_chars("Error: \u{1b}]0;pwned\u{7}x"),
            "Error: ]0;pwnedx"
        );
        // CSI cursor movement used to forge a fake status bar.
        assert_eq!(strip_control_chars("a\u{1b}[2Jb"), "a[2Jb");
        assert_eq!(strip_control_chars("bell\u{7}"), "bell");
        assert_eq!(strip_control_chars("cr\r"), "cr");
        assert_eq!(strip_control_chars("del\u{7f}"), "del");
        // C1 controls are multi-byte in UTF-8 and equally dangerous.
        assert_eq!(strip_control_chars("nel\u{85}"), "nel");
    }

    /// Ordinary text must be passed through untouched, and without allocating.
    #[test]
    fn ordinary_text_is_borrowed_unchanged() {
        let s = "fn main() { println!(\"日本語\"); }";
        let out = strip_control_chars(s);
        assert_eq!(out, s);
        assert!(
            matches!(out, std::borrow::Cow::Borrowed(_)),
            "clean text should not allocate"
        );
    }

    /// Tabs must survive `strip_control_chars`, since the layout expands them.
    #[test]
    fn tabs_are_preserved() {
        assert_eq!(strip_control_chars("a\tb"), "a\tb");
    }

    /// End-to-end: a buffer of hostile bytes must not put an ESC on the screen.
    /// This is the check that actually exercises ratatui's `Paragraph`, which --
    /// unlike `Buffer::set_stringn` -- does not filter control chars itself.
    #[test]
    fn hostile_file_content_never_reaches_the_terminal_buffer() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;
        use rune::tabs::TabManager;

        let mut tabs = TabManager::new_for_test();
        tabs.active_editor_mut().rope =
            ropey::Rope::from_str("A\u{1b}]0;pwned\u{7}B\rC\u{85}D\nsecond\n");

        let mut terminal = Terminal::new(TestBackend::new(40, 10)).unwrap();
        terminal.draw(|f| rune::ui::draw_ui(f, &mut tabs)).unwrap();

        let buffer = terminal.backend().buffer();
        for cell in buffer.content() {
            for ch in cell.symbol().chars() {
                assert!(
                    !ch.is_control() || ch == '\t',
                    "control char {:?} reached the terminal buffer",
                    ch
                );
            }
        }
    }

    /// Filenames are attacker-controlled too, and they reach the tab bar.
    #[test]
    fn hostile_filename_never_reaches_the_terminal_buffer() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;
        use rune::tabs::TabManager;

        let mut tabs = TabManager::new_for_test();
        tabs.active_editor_mut().display_name = "evil\u{1b}]0;pwned\u{7}.txt".to_string();

        let mut terminal = Terminal::new(TestBackend::new(60, 10)).unwrap();
        terminal.draw(|f| rune::ui::draw_ui(f, &mut tabs)).unwrap();

        for cell in terminal.backend().buffer().content() {
            for ch in cell.symbol().chars() {
                assert!(
                    !ch.is_control() || ch == '\t',
                    "control char {:?} from a filename reached the terminal buffer",
                    ch
                );
            }
        }
    }

    /// Stripping must not shift the column math the cursor relies on: control
    /// chars are zero-width in `char_display_width`, so dropping them keeps
    /// rendered columns and editor columns in agreement.
    #[test]
    fn stripping_preserves_display_width() {
        use rune::editor::str_display_width;
        let raw = "ab\u{1b}\u{7}cd";
        let stripped = strip_control_chars(raw);
        assert_eq!(str_display_width(raw, 0), str_display_width(&stripped, 0));
        assert_eq!(str_display_width(&stripped, 0), 4);
    }

    /// Guard the assumption the whole fix rests on: a bare ratatui `Paragraph`
    /// really does pass control chars straight through, so if a future ratatui
    /// version starts filtering them this test tells us the layer is redundant
    /// rather than silently doing nothing.
    #[test]
    fn paragraph_itself_does_not_filter_control_chars() {
        use ratatui::backend::TestBackend;
        use ratatui::widgets::Paragraph;
        use ratatui::Terminal;

        let mut terminal = Terminal::new(TestBackend::new(10, 1)).unwrap();
        terminal
            .draw(|f| {
                let p = Paragraph::new(Span::styled("A\u{1b}B", Style::default()));
                f.render_widget(p, f.area());
            })
            .unwrap();

        let found_esc = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .any(|c| c.symbol().contains('\u{1b}'));
        assert!(
            found_esc,
            "ratatui now filters control chars; rune's sanitization layer may be redundant"
        );
    }
}

/// `write_backup` streams rather than buffering, so a large file must not need a
/// second full copy in memory. Also a basic correctness check on large content.
#[test]
fn backup_handles_a_large_file() {
    use rune::config::Config;
    use rune::tabs::TabManager;

    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("big.txt");
    let mut f = std::fs::File::create(&target).unwrap();
    let line = "x".repeat(200);
    for _ in 0..5_000 {
        writeln!(f, "{line}").unwrap();
    }
    drop(f);
    let original_len = std::fs::metadata(&target).unwrap().len();

    let mut tabs = TabManager::new_for_test();
    tabs.config = Config {
        backup_on_save: true,
        ..Default::default()
    };
    tabs.active_editor_mut().rope = ropey::Rope::from_str("small\n");
    tabs.perform_save(target.clone()).unwrap();

    let backup = dir.path().join("big.txt~");
    assert_eq!(std::fs::metadata(&backup).unwrap().len(), original_len);
}
