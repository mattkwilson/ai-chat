import { FormEvent, KeyboardEvent, useState, useRef, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import "./App.css";

type ChatMessage = {
  role: "user" | "assistant" | "system";
  content: string;
};

function App() {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [input, setInput] = useState("");
  const [model, setModel] = useState("gemma4:e4b");
  const [models, setModels] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const messagesRef = useRef<HTMLDivElement>(null);
  const hasStartedRef = useRef<boolean>(false);
  const [thumbStyle, setThumbStyle] = useState({ top: 0, height: 0 });
  const resolveRef = useRef<(() => void) | null>(null);
  const rejectRef = useRef<((reason: string) => void) | null>(null);

  useEffect(() => {
    invoke<string[]>('list_models')
      .then((list) => {
        setModels(list);
        if (list.length > 0) setModel(list[0]);
      })
      .catch(() => {});
  }, []);

  useEffect(() => {
    let unlistenChunk: (() => void) | null = null;
    let unlistenDone: (() => void) | null = null;

    const setup = async () => {
      unlistenChunk = await listen('ollama-chunk', (event) => {
        const text = event.payload as string;
        if (!hasStartedRef.current) {
          if (!text.trim()) return;
          setMessages((current) => [...current, { role: "assistant", content: text }]);
          hasStartedRef.current = true;
        } else {
          setMessages((current) => {
            const msgs = [...current];
            msgs[msgs.length - 1].content = text;
            return msgs;
          });
        }
      });

      unlistenDone = await listen('ollama-done', () => {
        setLoading(false);
        resolveRef.current?.();
        resolveRef.current = null;
        rejectRef.current = null;
      });
    };

    setup();

    return () => {
      unlistenChunk?.();
      unlistenDone?.();
    };
  }, []);

  const updateThumb = () => {
    const el = messagesRef.current;
    if (!el) return;
    const { scrollTop, scrollHeight, clientHeight } = el;
    if (scrollHeight <= clientHeight) {
      setThumbStyle({ top: 0, height: 0 });
      return;
    }
    const height = (clientHeight / scrollHeight) * 100;
    const maxScrollTop = scrollHeight - clientHeight;
    const availableTrack = 100 - height;
    const top = maxScrollTop > 0 ? (scrollTop / maxScrollTop) * availableTrack : 0;
    setThumbStyle({
      top,
      height,
    });
  };

  useEffect(() => {
    const el = messagesRef.current;
    if (!el) return;
    el.scrollTop = el.scrollHeight;
    updateThumb();
  }, [messages, loading]);

  const sendMessage = async (event?: FormEvent<HTMLFormElement>) => {
    event?.preventDefault();
    const prompt = input.trim();
    if (!prompt) return;

    setError(null);
    setInput("");

    const userMessage: ChatMessage = { role: "user", content: prompt };
    setMessages((current) => [...current, userMessage]);
    setLoading(true);
    hasStartedRef.current = false;

    try {
      await new Promise<void>((resolve, reject) => {
        resolveRef.current = resolve;
        rejectRef.current = reject;
        invoke('ollama_chat', { prompt, model }).catch((err) => {
          rejectRef.current?.(err);
          rejectRef.current = null;
          resolveRef.current = null;
        });
      });
    } catch (err) {
      const message =
        typeof err === "string"
          ? err
          : err instanceof Error
          ? err.message
          : "Unable to contact local Ollama server.";
      setError(message);
      setLoading(false);
    }
  };

  const cancelMessage = () => {
    invoke('cancel_chat').catch(() => {});
    // backend will emit ollama-done which resolves the promise and clears loading
  };

  const clearChat = () => {
    setMessages([]);
    setError(null);
  };

  const handleTextareaKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      if (!loading) {
        sendMessage();
      }
    }
  };

  return (
    <main className="container">
      <div className="top-bar">
        {models.length > 0 && (
          <select
            className="model-select"
            value={model}
            onChange={(e) => setModel(e.target.value)}
            disabled={loading}
          >
            {models.map((m) => (
              <option key={m} value={m}>{m}</option>
            ))}
          </select>
        )}
        {messages.length > 0 && !loading && (
          <button type="button" className="clear-btn" onClick={clearChat} aria-label="Clear chat">
            <svg width="10" height="10" viewBox="0 0 10 10" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round">
              <line x1="1" y1="1" x2="9" y2="9" />
              <line x1="9" y1="1" x2="1" y2="9" />
            </svg>
            Clear
          </button>
        )}
      </div>
      <section className="chat-container">
        <div className="messages" ref={messagesRef} onScroll={updateThumb}>
          {messages.map((message, index) => (
            <div key={index} className={`message ${message.role}`}>
              <ReactMarkdown remarkPlugins={[remarkGfm]}>{message.content}</ReactMarkdown>
            </div>
          ))}
          {loading && messages.length > 0 && messages[messages.length - 1].role === 'user' && (
            <div className="message assistant">
              <div className="loading-dots">
                <span /><span /><span /><span /><span />
              </div>
            </div>
          )}
        </div>
        {thumbStyle.height > 0 && (
          <div className="scrollbar-track">
            <div
              className="scrollbar-thumb"
              style={{ top: `${thumbStyle.top}%`, height: `${thumbStyle.height}%` }}
            />
          </div>
        )}
      </section>

      <form className="input-row" onSubmit={sendMessage}>
        <textarea
          value={input}
          onChange={(event) => setInput(event.currentTarget.value)}
          onKeyDown={handleTextareaKeyDown}
          placeholder="Enter your message here"
          disabled={loading}
        />
        {loading && (
          <button type="button" className="cancel-btn" onClick={cancelMessage} aria-label="Stop">
            <svg width="10" height="10" viewBox="0 0 10 10" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round">
              <line x1="1" y1="1" x2="9" y2="9" />
              <line x1="9" y1="1" x2="1" y2="9" />
            </svg>
          </button>
        )}
      </form>

      {error ? <div className="error">{error}</div> : null}
    </main>
  );
}

export default App;
