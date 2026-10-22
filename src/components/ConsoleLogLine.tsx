const ansiColor = /\x1b\[[0-9;]*m/g;

export function plainConsoleLine(line: string) {
  return line.replace(ansiColor, "");
}

export function isChatConsoleLine(line: string) {
  const text = plainConsoleLine(line);
  const prefixEnd = text.indexOf("]:");
  if (prefixEnd < 0) return false;
  const message = text.slice(prefixEnd + 2).trimStart();
  return /^(?:\[(?:Not Secure|Secure)\]\s*)?<[^>]+>\s/.test(message);
}

export function ConsoleLogLine({ line, trailingNewline }: { line: string; trailingNewline: boolean }) {
  const text = plainConsoleLine(line);
  const timestamp = /^\[(\d{2}:\d{2}:\d{2})/.exec(text);
  const remainder = timestamp ? text.slice(timestamp[0].length) : text;
  const level = /\b(ERROR|WARN|INFO|DEBUG|TRACE)(?=\])/.exec(remainder);
  const errorContinuation = /^(?:\s+at\s|Caused by:|Suppressed:|\s*\.\.\. \d+ more|[\w.]+(?:Exception|Error):)/.test(text);
  const tone = isChatConsoleLine(text) ? "chat" : level?.[1] === "ERROR" || errorContinuation ? "error" : level?.[1] === "WARN" ? "warning" : "normal";

  return <span className={`console-line console-line-${tone}`}>
    {timestamp && <>[<span className="console-line-time">{timestamp[1]}</span></>}
    {level ? <>{remainder.slice(0, level.index)}<span className="console-line-level">{level[1]}</span>{remainder.slice(level.index + level[1].length)}</> : remainder}
    {trailingNewline && "\n"}
  </span>;
}
