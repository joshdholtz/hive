use hive::pty::output::OutputBuffer;

#[test]
fn output_buffer_tracks_screen_contents() {
    let mut buffer = OutputBuffer::new(5, 20, 100);
    buffer.push_bytes(b"hello world");

    let contents = buffer.screen().contents();
    assert!(contents.contains("hello world"));
}
