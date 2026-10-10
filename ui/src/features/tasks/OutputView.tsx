import { useEffect, useMemo, useRef, useState } from "react";
import { Button, Code, Group, Stack, Text, TextInput } from "@mantine/core";
import { OUTPUT_CHARACTER_LIMIT } from "./outputBuffer";

const ROW_HEIGHT = 20;
const HEIGHT = 320;
const OVERSCAN = 6;

/** Filtering is limited to the loaded display window; it does not change worker capture. */
export function OutputView({ text, label, empty, omittedCharacters = 0 }: { text: string; label: string; empty: string; omittedCharacters?: number }) {
  const [query, setQuery] = useState("");
  const [frozen, setFrozen] = useState<{ text: string; omitted: number } | null>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [following, setFollowing] = useState(true);
  const viewport = useRef<HTMLDivElement>(null);
  const display = frozen?.text ?? text;
  const omitted = frozen?.omitted ?? omittedCharacters;
  const matches = useMemo(() => { const lines = display.split("\n"); const needle = query.toLocaleLowerCase(); return needle ? lines.filter((line) => line.toLocaleLowerCase().includes(needle)) : lines; }, [display, query]);
  const start = Math.max(0, Math.min(matches.length - 1, Math.floor(scrollTop / ROW_HEIGHT)) - OVERSCAN);
  const end = Math.min(matches.length, start + Math.ceil(HEIGHT / ROW_HEIGHT) + OVERSCAN * 2);
  useEffect(() => {
    if (following && frozen === null && viewport.current) { viewport.current.scrollTop = viewport.current.scrollHeight; setScrollTop(viewport.current.scrollTop); }
  }, [display, query, following, frozen]);
  useEffect(() => { if (viewport.current) viewport.current.scrollTop = 0; setScrollTop(0); }, [query]);
  return <Stack gap="xs">
    <Group><TextInput label={`Фильтр ${label}`} value={query} onChange={(e) => setQuery(e.currentTarget.value)} placeholder="Строки, содержащие текст" /><Button size="xs" variant="light" onClick={() => setFrozen(frozen === null ? { text, omitted: omittedCharacters } : null)}>{frozen === null ? `Приостановить просмотр ${label}` : `Продолжить просмотр ${label}`}</Button><Button size="xs" variant="light" onClick={() => setFollowing(!following)}>{following ? "Остановить автопрокрутку" : "Следить за концом"}</Button></Group>
    <Text size="xs">Последние {OUTPUT_CHARACTER_LIMIT.toLocaleString("ru-RU")} символов загруженного вывода · за пределами просмотра: {omitted}{query ? ` · найдено строк: ${matches.length}` : ""}{frozen !== null ? " · просмотр приостановлен; worker продолжает собирать вывод" : ""}. Полный сохранённый вывод остаётся у worker.</Text>
    {display && matches.length ? <div ref={viewport} role="region" aria-label={`Вывод ${label}`} tabIndex={0} data-testid={`output-viewport-${label}`} data-total-lines={matches.length} style={{ height: HEIGHT, overflow: "auto", background: "var(--mantine-color-default-hover)", fontFamily: "monospace", fontSize: 13, lineHeight: `${ROW_HEIGHT}px` }} onScroll={(event) => {
      const node = event.currentTarget; setScrollTop(node.scrollTop);
      setFollowing(node.scrollTop + node.clientHeight >= node.scrollHeight - ROW_HEIGHT);
    }}>
      <div style={{ height: start * ROW_HEIGHT }} />
      {matches.slice(start, end).map((line, index) => <div key={start + index} data-output-row style={{ height: ROW_HEIGHT, whiteSpace: "pre", minWidth: "max-content", paddingInline: 8 }}>{line || "\u00a0"}</div>)}
      <div style={{ height: (matches.length - end) * ROW_HEIGHT }} />
    </div> : <Code block>{display ? "Нет строк по фильтру" : empty}</Code>}
  </Stack>;
}
