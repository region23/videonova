# YouTube Video Translator

A desktop application for translating YouTube videos. This application allows you to:

1. Download a YouTube video
2. Transcribe the audio to text using Whisper
3. Translate the text to another language
4. Generate speech from the translated text
5. Merge everything into a new video with translated audio and subtitles

## Features

- Download YouTube videos using yt-dlp
- Transcribe audio using OpenAI Whisper
- Translate text using OpenAI API
- Generate speech from translated text
- Merge audio, video, and subtitles using ffmpeg
- Cross-platform (macOS, Windows)
- Modern UI with Vuetify

## Requirements

- [Rust](https://www.rust-lang.org/tools/install)
- [Node.js](https://nodejs.org/en/download/)
- [pnpm](https://pnpm.io/installation)
- [ffmpeg](https://ffmpeg.org/download.html) (will be downloaded automatically if not found)

## Development

1. Clone the repository
2. Install dependencies:
   ```
   pnpm install
   ```
3. Run the development server:
   ```
   pnpm tauri dev
   ```

## Building

To build the application for production:

```
pnpm tauri build
```

## License

MIT
