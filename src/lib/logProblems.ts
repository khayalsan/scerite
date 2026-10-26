import { plainConsoleLine } from "../components/ConsoleLogLine";

export interface LogProblem {
  key: string;
  line: number;
  title: string;
  suggestions: string[];
}

export function detectLogProblems(text: string): LogProblem[] {
  const problems = new Map<string, LogProblem>();
  text.split(/\r?\n/).forEach((rawLine, index) => {
    const line = plainConsoleLine(rawLine);
    const plugin = /(?:Error occurred while enabling|Could not enable)\s+([^\s]+)|(?:plugin ['"]([^'"]+)['"] could not be enabled)/i.exec(line);
    let problem: Omit<LogProblem, "line"> | null = null;

    if (plugin) {
      const name = (plugin[1] || plugin[2]).replace(/\s+v[\d.]+.*$/i, "");
      problem = {
        key: `plugin:${name.toLowerCase()}`,
        title: `${name} could not be enabled`,
        suggestions: ["Check this plugin's version and server compatibility.", "Read the nearby error and stack trace for a missing dependency or configuration problem."],
      };
    } else if (/FAILED TO BIND TO PORT|Address already in use|Failed to bind to port/i.test(line)) {
      problem = {
        key: "port-in-use",
        title: "The server port is already in use",
        suggestions: ["Check whether another server is using this port.", "Choose a free server port in server.properties before starting again."],
      };
    } else if (/OutOfMemoryError|Could not reserve enough space for object heap/i.test(line)) {
      problem = {
        key: "out-of-memory",
        title: "The server ran out of memory",
        suggestions: ["Check the server's RAM setting and available memory on the host.", "Reduce memory use or increase the limit before restarting."],
      };
    } else if (/You need to agree to the EULA|eula=false/i.test(line)) {
      problem = {
        key: "eula",
        title: "The Minecraft EULA has not been accepted",
        suggestions: ["Review the EULA, then accept it in eula.txt if you agree."],
      };
    }

    if (problem) problems.set(problem.key, { ...problem, line: index + 1 });
  });
  return [...problems.values()].sort((left, right) => left.line - right.line);
}
