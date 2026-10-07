import { useState } from "react";
import { Alert, Badge, Box, Container, Group, MantineProvider, SegmentedControl, Stack, Text, Title } from "@mantine/core";
import { demoScenarios } from "../fixtures/demo";
import { TaskWorkbench } from "../features/tasks/TaskWorkbench";
import { NativeWorkbench } from "../features/tasks/NativeWorkbench";
import { ReportWorkbench } from "../features/tasks/ReportWorkbench";
import { nativeRuntime } from "../runtime/adapter";
import { AttentionPopup } from "../features/attention/AttentionPopup";
import { ValidationBanner } from "../features/attention/ValidationBanner";

export function App() {
  const [scenarioId, setScenarioId] = useState(() => new URLSearchParams(window.location.search).get("scenario") ?? "success");
  const scenario = demoScenarios.find((item) => item.id === scenarioId) ?? demoScenarios[0];
  if (nativeRuntime && new URLSearchParams(window.location.search).has("attention-popup")) return <MantineProvider defaultColorScheme="dark"><AttentionPopup /></MantineProvider>;
  return <MantineProvider defaultColorScheme="dark">
    {nativeRuntime && <ValidationBanner />}
    <Box className="page"><Container size="lg" py="xl"><Stack gap="xl">
      <Group justify="space-between" align="start">
        <Box><Text className="eyebrow">WINBRIDGE / DEBIAN ↔ WINDOWS</Text><Title order={1}>Каждое действие видно</Title><Text c="dimmed" mt="xs">Команды, файлы и результаты в одном журнале проекта</Text></Box>
        <Badge variant="outline" color={nativeRuntime ? "teal" : "gray"}>{nativeRuntime ? "Локальный worker" : "Браузер: демонстрация / отчёт"}</Badge>
      </Group>
      {nativeRuntime ? <NativeWorkbench /> : <>
      <ReportWorkbench />
      <Alert color="blue" title="Интерактивный прототип">
        Данные ниже — примеры. Worker не подключён; команды и файловые операции не выполняются.
      </Alert>
      <Stack gap="xs"><Text size="sm" fw={600}>Выберите сценарий</Text><SegmentedControl fullWidth value={scenarioId} onChange={setScenarioId} data={demoScenarios.map((item) => ({ value: item.id, label: item.label }))} /></Stack>
      <TaskWorkbench key={scenario.id} scenario={scenario} />
      </>}
      <Text size="xs" c="dimmed">{nativeRuntime ? "Команды и результаты сохраняются в локальном журнале worker" : "Схема доставки: /mnt/hgfs/shared ↔ K:\\VMs\\shared → отдельная папка сборки Windows"}</Text>
    </Stack></Container></Box>
  </MantineProvider>;
}
