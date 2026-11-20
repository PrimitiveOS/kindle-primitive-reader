# Reader for PrimitiveOS

Reader is a planned Kindle app for reading books and documents. The app will use
the PrimitiveOS Rust framework for the Kindle interface and an independently
implemented reading engine. KOReader provides a reference for supported formats,
reading workflows, and performance on the same device.

The first milestone is an EPUB feasibility check in the desktop simulator and on
the Kindle Oasis 3. The check will determine which rendering approach can show
real books, turn pages promptly, and restore the same passage after repagination.
The feasibility code and corpus support initial rendering and device
benchmarks. The reading app follows after the P0 acceptance checks.

## Design documents

| Document | Contents |
|---|---|
| [System architecture](docs/architecture.md) | Rendering, document positions, persistence, and Kindle integration |
| [Requirements and KOReader parity](docs/requirements.md) | Kindle workflows, parity scope, and acceptance criteria |
| [Execution plan](docs/execution-plan.md) | EPUB feasibility experiments, device verification, and release milestones |
| [EPUB probe](docs/epub-probe.md) | Commands for comparing the two renderers on desktop and Kindle |
| [EPUB feasibility results](docs/epub-probe-results.md) | Oasis 3 measurements, current engine choice, and remaining validation |

## Licence

Project-owned software uses [PolyForm Noncommercial 1.0.0](LICENSE). Project-owned
documentation and artwork use [CC BY-NC-SA 4.0](LICENSE-DOCS). Third-party
dependencies retain their own terms.
