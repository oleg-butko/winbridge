# Capabilities

Статически подключаемые handlers: runner, workspace-sync и log-reader реализованы
Cargo packages. Attention journal остаётся у worker/core, native presentation —
в desktop. [Контракты и следующий проект](../docs/guides/project-integration.md) описывают
inputs/policy, callbacks и единое владение journal/cancel/recovery. Не создавать
отдельную очередь для каждой capability и не вводить SDK без внешнего потребителя.
