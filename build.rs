fn main() {
    // Embed the app icon into the Windows executable
    #[cfg(target_os = "windows")]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("meeting-transcriber.ico");
        res.set("ProductName", "Meeting Transcriber");
        res.set("FileDescription", "Meeting Transcription and Analysis Tool");
        if let Err(e) = res.compile() {
            eprintln!("Warning: Failed to embed Windows icon: {e}");
        }
    }
}
