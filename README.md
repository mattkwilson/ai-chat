# AI Chat

A desktop AI chat app powered by your local [Ollama](https://ollama.com) models. No cloud, no data leaves your machine.

Built with [Tauri](https://tauri.app), React, and Rust.

## Features

- Streams responses in real-time as they're generated
- Automatically lists all locally downloaded Ollama models
- Markdown rendering — code blocks, tables, bold, etc.
- Cancel a response mid-stream

## Requirements

- [Node.js](https://nodejs.org) (v18+)
- [Rust](https://rustup.rs)
- [Ollama](https://ollama.com) running locally with at least one model pulled

## Getting Started

```bash
# Install dependencies
npm install

# Run in development mode
npm run tauri dev
```

To build a production app:

```bash
npm run tauri build
```

## Usage

1. Start Ollama: `ollama serve`
2. Pull a model if you haven't already: `ollama pull llama3`
3. Launch the app — your downloaded models will appear in the dropdown
4. Type a message and press **Enter** to send (Shift+Enter for a new line)

## Notes

- Ollama must be running on `http://127.0.0.1:11434`
