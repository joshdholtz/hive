use hive::pty::output::OutputBuffer;

#[test]
fn output_buffer_tracks_screen_contents() {
    let mut buffer = OutputBuffer::new(5, 20, 100);
    buffer.push_bytes(b"hello world");

    let contents = buffer.screen().contents();
    assert!(contents.contains("hello world"));
}

#[test]
fn output_buffer_scroll_offset_moves() {
    let mut buffer = OutputBuffer::new(5, 20, 10);
    buffer.push_bytes(b"line1\nline2\nline3\nline4\nline5\nline6\n");

    buffer.scroll_up(3);
    assert_eq!(buffer.scroll_offset(), 3);

    buffer.scroll_down(2);
    assert_eq!(buffer.scroll_offset(), 1);

    buffer.scroll_to_top();
    assert_eq!(buffer.scroll_offset(), 10);

    buffer.scroll_to_bottom();
    assert_eq!(buffer.scroll_offset(), 0);
}
