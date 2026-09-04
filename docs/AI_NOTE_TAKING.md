# Set up AI note taking

Shorthand records and transcribes on your computer. The note itself is written
by **Claude Code**, **Codex**, or **Cursor CLI**, installed on this computer and signed in to
your own subscription. Usage counts against that subscription; no API key is
needed. The assistant receives the transcript and the current note, never the
audio.

Do these steps once, before your first capture. Without them, capture and
transcription still work, but the note never appears.

## 1. Install the assistant and sign in

Open PowerShell (Windows) or Terminal (macOS, Linux). Sign in with the account
that carries your subscription; the assistant remembers it.

**Claude Code** (Claude Pro and Max)

1. Follow [Install Claude Code](https://code.claude.com/docs/en/setup#install-claude-code).
2. Sign in:

   ```sh
   claude auth login
   ```

**Codex** (ChatGPT Plus and Pro)

1. Follow [Codex CLI: getting started](https://learn.chatgpt.com/docs/codex/cli#getting-started).
2. Sign in:

   ```sh
   codex login
   ```

**Cursor CLI** (Cursor subscription)

1. Follow [Install Cursor CLI](https://cursor.com/cli).
2. Sign in:

   ```sh
   agent login
   ```

## 2. Connect Obsidian

1. In Shorthand, open **Notes** and choose **Install in Obsidian**. Obsidian
   opens on the Shorthand plugin's page.
2. In Obsidian, choose **Install**, then **Enable**. Back in Shorthand, the
   Notes section now shows the plugin as installed.
3. In Obsidian, open the Shorthand plugin's settings and set **Enhancement
   backend** to the assistant from step 1: **Claude Code** (the default),
   **Codex**, or **Cursor CLI**. You can also connect to an **Agent Client Protocol (ACP)**
   agent, or choose **LLM provider** if you would rather use an API
   key, a local model (such as Ollama), or your own OpenAI-compatible endpoint.

If Obsidian is not installed yet, the Notes section says so and offers a link
to get it.

## 3. Take your first note

1. Keep Shorthand running and open a Markdown note in Obsidian.
2. Open the command palette and run **Shorthand: Start meeting capture on this note**.
3. Talk normally. The note fills in as the conversation goes.
4. Run **Shorthand: Stop capture** when you finish.

For a solo session, turn on **Assisted notes** in Shorthand under
**Modes → Notetaking**, then run **Shorthand: Start assisted notes capture on
this note** in Obsidian.

The plugin writes only inside its own marked section. Anything you type outside
that section stays as it is.

## If the note never appears

Work down this list; the first item that fails is the cause.

1. **Is the assistant installed and signed in?** In a new terminal window, run
   `claude auth status` or `codex login status`. A "not found" error means the
   install did not finish, or the terminal was opened before it did.
2. **Does the plugin point at the one you installed?** Check **Enhancement
   backend** in the plugin's settings in Obsidian.
3. **Does Notes in Shorthand show the plugin as installed and enabled?** If
   not, choose **Install in Obsidian** again.
4. **Is the transcript empty?** Open **History** in Shorthand. An empty
   transcript is a microphone or model problem, not an assistant problem;
   check **Capture** and **Transcription** in the app's settings.
