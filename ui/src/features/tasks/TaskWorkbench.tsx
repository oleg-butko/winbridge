import { useState } from "react";
import { Alert, Badge, Box, Button, Card, Code, Grid, Group, Stack, Tabs, Text, Timeline, Title } from "@mantine/core";
import type { DemoScenario } from "../../fixtures/demo";

const executable = "cargo.exe";
const argumentsList = ["build", "--locked", "--release"];
const workingDirectory = "K:\\WinBridge\\workspaces\\sample-app\\snapshots\\demo-001";

export function TaskWorkbench({ scenario }: { scenario: DemoScenario }) {
  const [copyState, setCopyState] = useState<string | null>(null);
  async function copyCommand() {
    try {
      await navigator.clipboard.writeText(JSON.stringify({ executable, arguments: argumentsList, workingDirectory }, null, 2));
      setCopyState("Параметры скопированы");
    } catch {
      setCopyState("Копирование недоступно; параметры можно выделить ниже");
    }
  }

  return <Stack gap="lg">
    <Group justify="space-between">
      <Box><Text size="sm" c="dimmed">sample-app / task-demo-001</Text><Title order={2}>Сборка проекта на Windows</Title></Box>
      <Badge color={scenario.color} size="lg">{scenario.status}</Badge>
    </Group>
    <Alert color={scenario.color} title={scenario.exitCode === null ? "Результат ещё не подтверждён" : `Exit code: ${scenario.exitCode}`}>
      {scenario.explanation}
    </Alert>
    <Grid>
      <Grid.Col span={{ base: 12, md: 4 }}>
        <Card withBorder h="100%"><Stack gap="sm">
          <Text fw={600}>Контекст выполнения</Text>
          <Text size="sm">Инициатор: Debian agent</Text>
          <Text size="sm">Рецепт: build-release</Text>
          <Text size="sm">Snapshot: demo-001</Text>
          <Text size="sm">Timeout: 10 минут</Text>
          <Text size="sm">Доставка: VMware Shared Folders</Text>
          <Text size="sm" c="dimmed">Это пример сохранённой задачи. Реальное исполнение появится после подключения worker.</Text>
        </Stack></Card>
      </Grid.Col>
      <Grid.Col span={{ base: 12, md: 8 }}>
        <Card withBorder><Stack gap="md">
          <Group justify="space-between"><Text fw={600}>Фактические параметры команды</Text><Button variant="light" size="xs" onClick={() => void copyCommand()}>Копировать параметры</Button></Group>
          <Text size="sm">Executable</Text><Code block>{executable}</Code>
          <Text size="sm">Аргументы — отдельные значения</Text><Code block>{JSON.stringify(argumentsList)}</Code>
          <Text size="sm">Рабочая папка</Text><Code block>{workingDirectory}</Code>
          {copyState && <Text size="xs" c="dimmed" role="status">{copyState}</Text>}
        </Stack></Card>
      </Grid.Col>
    </Grid>
    <Tabs defaultValue={new URLSearchParams(window.location.search).get("tab") === "output" ? "output" : "timeline"}>
      <Tabs.List><Tabs.Tab value="timeline">Ход выполнения</Tabs.Tab><Tabs.Tab value="output">stdout / stderr</Tabs.Tab></Tabs.List>
      <Tabs.Panel value="timeline" pt="lg">
        <Card withBorder><Timeline active={scenario.events.length - 1} bulletSize={24} lineWidth={2}>
          {scenario.events.map((event) => <Timeline.Item key={event.time} title={event.title} color={event.color}>
            <Text size="sm" c="dimmed">{event.detail}</Text><Text size="xs" mt={4}>{event.time} · демонстрационное время</Text>
          </Timeline.Item>)}
        </Timeline></Card>
      </Tabs.Panel>
      <Tabs.Panel value="output" pt="lg"><Card withBorder><Text size="sm" c="dimmed" mb="sm">Пример сохранённого вывода выбранного сценария</Text><Code block>{scenario.output}</Code></Card></Tabs.Panel>
    </Tabs>
  </Stack>;
}
