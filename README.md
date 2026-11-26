[English](README.md) | [Русский](README.ru.md)

# EvoHime (Eva)

### Local AI Agent for Windows — an AI Coding Assistant That Sees Tasks Through to Completion

**EvoHime** (Eva) is a **Local AI Agent for Windows** built on a **Rust + Electron** architecture with a secure **Supervised Runtime**. Eva understands your workspace context, helps diagnose problems, proposes clear solutions, and executes actions alongside you with explicit approval gates.

More than a basic chat window, EvoHime works as a structured **AI Coding Assistant** where every task has transparent progress, verifiable outcomes, and strict control over sensitive operations.

## What You Can Delegate to Eva

- explore codebases, documentation, and project structure;
- find root causes of bugs and propose targeted fixes;
- edit files and verify the results;
- draft implementation plans and gather multiple independent reviews;
- continue work across saved chat histories;
- track persistent memory, background tasks, and scheduled runs;
- inspect open issues and pull requests in public GitHub repositories;
- assist with project recovery when something goes wrong.

## Getting Started

1. [Download the lightweight EvoHime-Setup.exe](https://github.com/rkfsociety/EvoHime/releases/download/installer/EvoHime-Setup.exe). It fetches the verified module bundle during initial installation or repair.
2. Install the app and launch Eva from your desktop.
3. Select your project workspace folder.
4. Open provider settings and choose a source and model — connect an API profile, **Google Gemini**, local **Ollama**, or local Codex CLI.
5. Describe your task in plain language.

Eva displays every step of execution. Before editing files, running critical actions, or performing other sensitive operations, it always asks for your confirmation.

You can switch the interface language in appearance settings between English and Russian. The choice is saved locally, while chat messages remain in whatever language the selected model uses.

## Architecture: Rust + Electron & Supervised Runtime

EvoHime separates UI rendering from execution and state ownership:

- **Electron Desktop UI** — responsive desktop interface that renders live task state over an authenticated local named pipe;
- **Rust Core Runtime** — owns workspace access, tool execution, approval checks, and local SQLite storage;
- **Supervised Runtime** — a dedicated Windows supervisor process manages lifecycle, Job Object isolation, and automatic recovery if the core crashes.

## Providers and Models

Supported backends include **Ollama**, **OpenAI-compatible API**, OpenAI Responses API, LiteRouter, and local Codex CLI. Available models depend on the selected provider and refresh directly inside the app. For regular chats, you can switch models right before starting a task.

For project recovery, select a provider and model in advance. Eva never triggers self-repair automatically: launch, edits, publishing, and restarts each require separate confirmation.

## Scheduling and Workspace Overview

The **Scheduled** button shows local recurring and one-shot tasks along with timing, owner, status, and last run. The workspace overview surfaces memory, Pulse, child tasks, tools, and overall project state.

## Privacy and Security

- workspace data and chat history stay locally on your machine;
- API keys are protected with Windows native encryption (DPAPI) and are never displayed in the UI;
- sensitive operations always require explicit user confirmation;
- Eva never modifies your project or starts recovery without your action;
- when using a cloud model, sent context is handled under the selected provider's terms;
- GitHub integration works only with public repositories and sends requests only after you select or refresh a repository in settings.

## System Requirements

- Windows 10 version 2004 or later, or Windows 11;
- 64-bit (x64) system.

No external developer runtimes are required for regular use. The single web installer `EvoHime-Setup.exe` handles both initial installation and repair by downloading the verified updater and a compatible set of modules from GitHub.
[Download installer](https://github.com/rkfsociety/EvoHime/releases/tag/installer)

## Support the Project

If you would like to support EvoHime development, you can send USDT via either network below.

### Ethereum (ERC20)

`0x806f0f84059d506ca017501c504693509680960d`

### TON

`UQDnW88gMYJgD3J3sjuoFYctgKhn6ArisE8mnMCQfnYQ9UBx`

Please verify that you have selected the correct network before sending.

## Contributing

You can submit ideas and reproducible bug reports via [GitHub Issues](https://github.com/rkfsociety/EvoHime/issues). Before posting, please review the [Contributing Guide](CONTRIBUTING.md), [Code of Conduct](CODE_OF_CONDUCT.md), and [Security Policy](SECURITY.md). Do not post API keys, tokens, personal data, or working exploits in issues.

## License

This project is licensed under the [MIT License](LICENSE) — free to use, modify, and distribute. The software is provided "as is", without warranty of any kind.
