# Issue tracker: Local Markdown

Issues and specs live in `.agents/matt-pocock/`.

- One feature per directory: `.agents/matt-pocock/<feature-slug>/`.
- Spec: `<feature-directory>/spec.md`.
- Tickets: `<feature-directory>/issues/<NN>-<slug>.md`, numbered from 01.
- Record triage state in a `Status:` line near the top of each ticket.
- Append discussion under `## Comments`.

When publishing a spec or issue, create its file and parent directories.
When fetching a ticket, read the path or numbered ticket provided by the user.

## Wayfinding

- Store the map at `.agents/matt-pocock/<effort>/map.md`.
- Store child tickets in its `issues/` directory.
- Record ticket type in `Type:` and state in `Status:`.
- Record dependencies as `Blocked by: NN, NN`.
- Work on open, unblocked, unclaimed tickets in number order.
- Claim a ticket by setting `Status: claimed`.
- Resolve it by adding `## Answer`, setting `Status: resolved`, and adding
  a summary and link to the map's Decisions-so-far section.
