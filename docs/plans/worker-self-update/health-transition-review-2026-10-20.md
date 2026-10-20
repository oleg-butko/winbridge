# Ревью health-driver и базовый native gate

2026-10-20. Базовый gate clean `3ca392ad7726faf2ff38a208999f8566222eaeaf`
прошёл: run `20261020-053153-bde36139`, Task
`e3a474d6-48a2-4460-bd6d-4d43d377a44a`, succeeded / exit 0,
217 native tests / 19 groups, полный build четырёх exe и реальные
config/rehearsal/Main/pins. Все 47 ready entries проверены по размеру/SHA.
SHA `report.json`: `0975a1ff2ca28a8193048ef88ae45911b27cd3770d7238240b4d404a60d0d3e7`.
Windows free before/after: 20,118,642,688 / 19,724,242,944 bytes.
Установленный d052 worker connected, active tasks 0, прежний generation
`1a3f242c-7da5-4daf-82fd-eb65566dc7cd`, recovery/maintenance errors нет.
GUI не запускался; worker оставить работать, desktop не нужен.

Ревью перед сквозной проверкой нашло существенную ошибку. Общий retained-transition
validator, ранее использовавшийся baseline/preimages, ожидал Requested → Requested.
Health journal использовал его для ValidatingCandidate → ValidatingCandidate и
ValidatingCandidate → CandidateVerified. Поэтому правильный health intent был бы
отклонён. Базовые DTO/raw-bypass/migration tests этого не покрывали; пройденный
native gate не является доказательством успешной health orchestration.

Validator теперь задаёт предыдущую фазу по конкретному разрешённому driver event,
а следующую вычисляет общей core FSM. Неподдерживаемый event, неверные key/owner/
epoch, фазы и snapshot по-прежнему отклоняются. Не добавлен общий обход проверок.
Новый тест пишет реальные root/transition rows обоих health edges и проверяет
положительные и отрицательные случаи. Это проверка SQLite/FSM, а не здоровья exe.
21 baseline/preimages/transition regression passed локально. Следующая проверка —
реальный полный candidate pipeline и health actor в изолированной Windows папке.
