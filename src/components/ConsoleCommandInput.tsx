import { useId, useState } from "react";
import type { FormEvent, KeyboardEvent } from "react";

const commonCommands = [
  "help", "list", "say", "tell", "me", "stop", "save-all", "save-on", "save-off",
  "op", "deop", "kick", "ban", "pardon", "ban-ip", "pardon-ip", "banlist",
  "whitelist", "whitelist add", "whitelist remove", "whitelist list", "whitelist on", "whitelist off",
  "gamemode", "gamerule", "difficulty", "time", "weather", "tp", "teleport",
  "give", "effect", "clear", "kill", "summon", "setworldspawn", "worldborder",
  "seed", "reload", "data", "scoreboard", "execute", "fill", "setblock",
  "locate", "playsound", "title", "advancement", "experience", "xp",
];

type Props = {
  disabled: boolean;
  onSend: (command: string) => Promise<boolean>;
};

export function ConsoleCommandInput({ disabled, onSend }: Props) {
  const listId = useId();
  const [input, setInput] = useState("");
  const [recentCommands, setRecentCommands] = useState<string[]>([]);
  const [suggestionsOpen, setSuggestionsOpen] = useState(false);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [sending, setSending] = useState(false);
  const query = input.trimStart();
  const slash = query.startsWith("/") ? "/" : "";
  const commandQuery = query.slice(slash.length).toLowerCase();
  const suggestions = query
    ? [...new Set([...recentCommands, ...commonCommands])]
      .filter((command) => command.toLowerCase().startsWith(commandQuery) && `${slash}${command}`.toLowerCase() !== query.toLowerCase())
      .slice(0, 8)
    : [];
  const showSuggestions = suggestionsOpen && suggestions.length > 0 && !disabled && !sending;

  function complete(command: string) {
    setInput(`${slash}${command}`);
    setSuggestionsOpen(false);
    setSelectedIndex(0);
  }

  function handleKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    if (event.key === "Escape" && showSuggestions) {
      event.preventDefault();
      setSuggestionsOpen(false);
    } else if ((event.key === "ArrowDown" || event.key === "ArrowUp") && suggestions.length > 0) {
      event.preventDefault();
      setSuggestionsOpen(true);
      setSelectedIndex((current) => !showSuggestions
        ? event.key === "ArrowDown" ? 0 : suggestions.length - 1
        : (current + (event.key === "ArrowDown" ? 1 : -1) + suggestions.length) % suggestions.length);
    } else if (event.key === "Tab" && showSuggestions) {
      event.preventDefault();
      complete(suggestions[selectedIndex] ?? suggestions[0]);
    }
  }

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const command = input.trim();
    if (!command || disabled || sending) return;
    setSending(true);
    setSuggestionsOpen(false);
    try {
      if (await onSend(command)) {
        setRecentCommands((current) => [command.replace(/^\//, ""), ...current.filter((item) => item !== command.replace(/^\//, ""))].slice(0, 20));
        setInput("");
      }
    } finally {
      setSending(false);
    }
  }

  return <div className="console-command-entry">
    <form className="console-input-row" onSubmit={(event) => void submit(event)}>
      <span aria-hidden="true">&gt;</span>
      <input
        role="combobox"
        aria-label="Console command"
        aria-autocomplete="list"
        aria-expanded={showSuggestions}
        aria-controls={showSuggestions ? listId : undefined}
        aria-activedescendant={showSuggestions ? `${listId}-${selectedIndex}` : undefined}
        value={input}
        onChange={(event) => { setInput(event.currentTarget.value); setSelectedIndex(0); setSuggestionsOpen(true); }}
        onFocus={() => setSuggestionsOpen(true)}
        onBlur={() => setSuggestionsOpen(false)}
        onKeyDown={handleKeyDown}
        placeholder="Enter a server command"
        disabled={disabled || sending}
      />
      <button className="button button-primary" type="submit" disabled={disabled || sending || !input.trim()}>Send</button>
    </form>
    {showSuggestions && <div className="console-command-suggestions" id={listId} role="listbox" aria-label="Command suggestions">
      {suggestions.map((command, index) => <div
        id={`${listId}-${index}`}
        className={`console-command-suggestion${index === selectedIndex ? " console-command-suggestion-active" : ""}`}
        role="option"
        aria-selected={index === selectedIndex}
        key={command}
        onMouseDown={(event) => event.preventDefault()}
        onClick={() => complete(command)}
      >{slash}{command}</div>)}
      <small>↑ ↓ select · Tab complete · Enter send</small>
    </div>}
  </div>;
}
