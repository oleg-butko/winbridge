// Clearly labelled fixtures for UX exploration. No live agent data.
export interface DemoScenario {
  id: string;
  label: string;
  status: string;
  color: string;
  exitCode: number | null;
  explanation: string;
  events: Array<{ time: string; title: string; detail: string; color: string }>;
  output: string;
}

const preparation = [
  { time: "19:20:01", title: "Задача принята", detail: "Инициатор: Debian agent · проект sample-app · task-demo-001", color: "blue" },
  { time: "19:20:02", title: "Снимок доставлен через общую папку", detail: "Manifest и hashes проверены. Исходники размещены в отдельной папке Windows.", color: "teal" },
  { time: "19:20:03", title: "Команда запущена", detail: "cargo.exe · build --locked --release · timeout 10 минут", color: "blue" },
];

export const demoScenarios: DemoScenario[] = [
  {
    id: "success", label: "Успех", status: "Завершено", color: "teal", exitCode: 0,
    explanation: "Сборка завершилась. Можно изучить команду, output и последовательность событий.",
    events: [...preparation, { time: "19:20:42", title: "Сборка завершена", detail: "Exit code 0 · результат: sample-app.exe", color: "teal" }],
    output: "   Compiling sample-app v0.1.0\n    Finished release profile in 39.0s\nArtifact: sample-app.exe\nExit code: 0",
  },
  {
    id: "failure", label: "Ошибка сборки", status: "Ошибка", color: "red", exitCode: 101,
    explanation: "Ошибка на шаге сборки. Запуск приложения не выполнялся. Повтор должен создавать новую задачу.",
    events: [...preparation, { time: "19:20:09", title: "Сборка не прошла", detail: "Exit code 101 · ошибка компилятора. Следующий шаг не запущен.", color: "red" }],
    output: "   Compiling sample-app v0.1.0\nerror[E0425]: cannot find value `settings` in this scope\n  --> src/main.rs:12:5\nerror: could not compile sample-app\nExit code: 101",
  },
  {
    id: "unknown", label: "Связь потеряна", status: "Нет свежего подтверждения", color: "yellow", exitCode: null,
    explanation: "Последнее подтверждение — запуск команды. Обрыв связи не означает остановку процесса. Нужно запросить состояние той же задачи после восстановления.",
    events: [...preparation, { time: "19:20:05", title: "Соединение с controller потеряно", detail: "Результат пока неизвестен. Автоматический повтор команды не выполняется.", color: "yellow" }],
    output: "   Compiling sample-app v0.1.0\n\n[Последний полученный фрагмент. Новые данные пока недоступны.]",
  },
];
