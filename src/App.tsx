import { useEffect } from "react";
import { ChatView } from "./components/ChatView";
import { ContextPanel } from "./components/ContextPanel";
import { SettingsView } from "./components/SettingsView";
import { Sidebar } from "./components/Sidebar";
import { bindEvents, useStore } from "./store";

export default function App() {
  const { view, sidebarOpen, setSidebarOpen, loadSessions, loadSettings, newSession } = useStore();

  useEffect(() => {
    const unbind = bindEvents();
    void loadSettings();
    void loadSessions();
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "n") {
        e.preventDefault();
        void newSession();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => {
      unbind();
      window.removeEventListener("keydown", onKey);
    };
  }, [loadSessions, loadSettings, newSession]);

  return (
    <div className={`app-shell ${view === "chat" ? "with-panel" : ""}`}>
      <Sidebar />
      {sidebarOpen && <div className="sidebar-scrim" onClick={() => setSidebarOpen(false)} />}
      {view === "chat" ? <ChatView /> : <SettingsView />}
      {view === "chat" && <ContextPanel />}
    </div>
  );
}
