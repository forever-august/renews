---
mode: agent
---
#codebase Refactor for readability and to reduce duplication.

Test code may have a reasonable amount of duplication.

Search for and fix low-hanging fruit for performance bottlenecks. If there is a bottleneck which requires larger-scale changes, do not fix but write a description of the problem and a plan for how to address it later.

When looking for performance bottlenecks consider both runtime performance and memory use. We are expecting high volumes of articles so we want to use streaming operations where possible to reduce how many are stored in memory at once.