import { useState } from "react";
import { Button, Code, Group, Stack, Text, TextInput } from "@mantine/core";

/** Filtering is limited to the loaded display window; it does not change worker capture. */
export function OutputView({ text, label, empty }: { text: string; label: string; empty: string }) {
  const [query, setQuery] = useState("");
  const [frozen, setFrozen] = useState<string | null>(null);
  const display = [...(frozen ?? text)].slice(-65536).join("");
  const lines = display.split("\n");
  const matches = query ? lines.filter((line) => line.toLocaleLowerCase().includes(query.toLocaleLowerCase())) : lines;
  return <Stack gap="xs">
    <Group><TextInput label={`Фильтр ${label}`} value={query} onChange={(e) => setQuery(e.currentTarget.value)} placeholder="Строки, содержащие текст" /><Button size="xs" variant="light" onClick={() => setFrozen(frozen === null ? text : null)}>{frozen === null ? `Приостановить просмотр ${label}` : `Продолжить просмотр ${label}`}</Button></Group>
    <Text size="xs">Последние 65 536 символов загруженного вывода{query ? ` · найдено строк: ${matches.length}` : ""}{frozen !== null ? " · просмотр приостановлен; worker продолжает собирать вывод" : ""}</Text>
    <Code block>{display ? matches.join("\n") || "Нет строк по фильтру" : empty}</Code>
  </Stack>;
}
