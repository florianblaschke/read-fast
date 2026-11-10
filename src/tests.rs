use super::*;

#[test]
fn cli_requires_a_path() -> Result<(), clap::Error> {
    assert!(Args::try_parse_from(["read-fast"]).is_err());
    let args = Args::try_parse_from(["read-fast", "my notes.md"])?;
    assert_eq!(args.path, PathBuf::from("my notes.md"));
    Ok(())
}

#[test]
fn pause_stops_advancement_and_resume_continues() -> io::Result<()> {
    let mut app = Reader::new("one two")?;
    app.key(KeyCode::Char('j'));
    assert!(!app.tick());
    assert_eq!(app.index, 0);
    app.key(KeyCode::Char('j'));
    assert!(!app.tick());
    assert_eq!(app.index, 1);
    assert!(app.tick());
    assert_eq!(app.index, 1);
    Ok(())
}

#[test]
fn completion_holds_last_word_and_replay_preserves_speed() -> io::Result<()> {
    let mut app = Reader::new("one two")?;
    app.key(KeyCode::Char('l'));
    app.tick();
    app.tick();
    assert!(app.finished);
    assert_eq!(app.index, 1);
    app.tick();
    assert_eq!(app.index, 1);
    app.key(KeyCode::Char('j'));
    assert!(app.finished);
    app.key(KeyCode::Char('r'));
    assert!(!app.finished);
    assert!(!app.paused);
    assert_eq!(app.index, 0);
    assert_eq!(app.delay_ms, 990);
    Ok(())
}

#[test]
fn replay_restarts_paused_and_single_word_readers() -> io::Result<()> {
    let mut app = Reader::new("one")?;
    app.tick();
    assert!(app.finished);
    app.key(KeyCode::Char('R'));
    assert!(!app.finished);
    app.key(KeyCode::Char('j'));
    app.key(KeyCode::Char('r'));
    assert!(!app.paused);
    assert_eq!(app.index, 0);
    Ok(())
}

#[test]
fn speed_is_bounded_and_quit_is_handled() -> io::Result<()> {
    let mut app = Reader::new("word")?;
    for _ in 0..300 {
        app.key(KeyCode::Char('l'));
    }
    assert_eq!(app.delay_ms, 10);
    for _ in 0..300 {
        app.key(KeyCode::Char('h'));
    }
    assert_eq!(app.delay_ms, 2000);
    assert!(app.key(KeyCode::Char('q')));
    Ok(())
}

#[test]
fn empty_input_is_rejected() {
    assert!(Reader::new(" \n\t").is_err());
}

#[test]
fn splits_lines_and_tabs_without_empty_words() {
    assert_eq!(
        split_to_words("one\n\n two\tthree "),
        vec!["one", "two", "three"]
    );
}

#[test]
fn middle_grapheme_is_red_and_anchored_at_center() -> io::Result<()> {
    for width in [5, 20, 21] {
        for (word, middle) in [
            ("a", "a"),
            ("cat", "a"),
            ("read", "a"),
            ("abcdefghijklmno", "h"),
            ("a界b", "界"),
            ("界ab", "a"),
            ("ae\u{301}b", "e\u{301}"),
        ] {
            let backend = ratatui::backend::TestBackend::new(width, 9);
            let mut terminal = ratatui::Terminal::new(backend)?;
            let app = Reader::new(word)?;
            terminal.draw(|frame| draw(frame, &app))?;
            let buffer = terminal.backend().buffer();
            let red: Vec<_> = (0..9)
                .flat_map(|y| (0..width).map(move |x| (x, y)))
                .filter(|&(x, y)| buffer[(x, y)].fg == Color::Red)
                .collect();
            assert_eq!(red.len(), 1, "{word} at width {width}");
            assert_eq!(red[0].0, width / 2, "{word}");
            assert_eq!(buffer[red[0]].symbol(), middle);
        }
    }
    Ok(())
}

#[test]
fn test_wpm() {
    assert_eq!(60, wpm(1000));
}

#[test]
fn renders_word_and_status_even_in_small_terminals() -> io::Result<()> {
    for (width, height) in [(80, 24), (10, 3), (1, 1)] {
        let backend = ratatui::backend::TestBackend::new(width, height);
        let mut terminal = ratatui::Terminal::new(backend)?;
        let app = Reader::new("hello world")?;
        terminal.draw(|frame| draw(frame, &app))?;
        if width == 80 {
            let text: String = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(ratatui::buffer::Cell::symbol)
                .collect();
            assert!(text.contains("hello"));
            assert!(text.contains("60"));
            assert!(text.contains("Playing"));
        }
    }
    Ok(())
}
