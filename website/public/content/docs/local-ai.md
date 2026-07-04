# Local AI Powered

In the era of cloud-based AI, sending your personal thoughts, journals, and private data to third-party servers is a massive privacy risk. Synabit solves this by integrating deeply with **Local AI**.

By connecting Synabit to a local LLM runner (like Ollama or LM Studio), you can leverage the power of Artificial Intelligence entirely on your own machine. Your data never leaves your laptop.

---

## How to Setup Local AI

To get started, you need to have a local LLM running on your machine. We highly recommend **Ollama** due to its ease of use.

### Step 1: Install Ollama
1. Download and install [Ollama](https://ollama.com).
2. Open your terminal and run a lightweight model, such as Llama 3 or Mistral:
   ```bash
   ollama run llama3
   ```
3. Keep Ollama running in the background.

### Step 2: Connect Synabit
1. Open Synabit and go to **Settings > Local AI**.
2. Enable the **Use Local LLM** toggle.
3. Enter the API URL for your local runner. (For Ollama, the default is usually `http://localhost:11434`).
4. Select the model you downloaded from the dropdown list.
5. Click **Test Connection**. Once successful, Synabit is now AI-powered!

---

## AI Features in Synabit

Once connected, Synabit acts as a co-pilot for your digital brain.

### Contextual Chat
Open the AI Assistant sidebar to chat with the model. Unlike generic ChatGPT, Synabit's assistant can "read" the note you currently have open. You can ask it to:
- "Summarize this article for me."
- "Find the action items hidden in this meeting note."
- "Rewrite this paragraph to be more professional."

### Brainstorming & Outlining
Stuck on a blank page? Type `/ai` in the editor to bring up the inline prompt. Ask it to generate an outline, draft an email, or brainstorm ideas based on your tags. The generated text is inserted directly into your note.

### Semantic Search (Coming Soon)
Soon, Local AI will power Synabit's search engine, allowing you to search your vault by *meaning* rather than exact keywords.
