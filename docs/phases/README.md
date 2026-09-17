# Phase records

One document per phase, written **during** the phase rather than after it.

Each records what was built, what was measured, what the measurements changed
about the plan, and what was deliberately left undone. The last of these matters
most: a phase that does not state its own gaps is a phase that hides them.

Measurements carry their conditions — machine, corpus, warm or cold cache.
A number without its conditions is not evidence.

| phase | subject | status |
|---|---|---|
| [0](phase-0-spike.md) | Spike — do the core ideas hold? | ✅ complete, all criteria passed |
| [1](phase-1-engine.md) | The engine | ✅ complete — kill criterion passed |
| [2](phase-2-correctness.md) | Adversarial correctness | ✅ complete — 4 defects found |
| [3](phase-3-networking.md) | Networking at scale | 🔨 built — kill criterion unmeasured |
| [4](phase-4-product.md) | Desktop product | 🔨 in progress — the daemon runs |
| [5](phase-5-mobile.md) | Mobile | 🔨 in progress — builds, never run on a phone |
| 6 | Commercial | not started |

Phase definitions and kill criteria: [../roadmap.md](../roadmap.md).
