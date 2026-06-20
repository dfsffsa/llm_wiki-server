# Frontend System Prompt Short

Use this as a compact frontend policy for Codex.

You are building for an indie product with a low-cost, low-complexity architecture.
Do not default to a single heavy frontend architecture for the whole site.

## Page Classification

Before writing frontend code, classify the page:

- `Layer A`: public content pages
  - examples: landing page, pricing, blog, docs, legal pages
- `Layer B`: rich interactive workspace
  - examples: editors, builders, diagram tools, canvases
- `Layer C`: authenticated app screens
  - examples: dashboard, account, billing, reports

## Rules

### Layer A

- Default to server-rendered or static HTML.
- Keep JavaScript minimal or zero.
- Do not build a SPA for these pages unless explicitly required by the existing codebase.
- Prioritize first paint, SEO, and low memory usage.

### Layer B

- A SPA is acceptable, but only inside this narrow route or route group.
- Keep rich client-side logic local to the workspace.
- Do not let editor/workspace requirements dictate the architecture of the whole site.
- Push heavy computation to the backend whenever possible.

### Layer C

- Use moderate client-side interactivity only where needed.
- Keep backend as the source of truth.
- Prefer server-shaped data over large client-side data processing pipelines.
- Do not rebuild backend business logic in the browser.

## Backend-First Rule

Move these to the backend by default:

- AI inference
- ranking/scoring pipelines
- large data aggregation
- heavy rendering
- document parsing
- export generation
- expensive search transformations

The browser should primarily handle:

- input
- interaction
- presentation
- small local state

## State Management

- Prefer local state first.
- Use a global store only for real shared needs such as:
  - current user
  - feature flags
  - tiny global preferences
- Do not create large global state for simple pages or forms.

## Routing

- Public content routes should stay simple and crawlable.
- Client-side routing is acceptable for authenticated app areas and workspaces.
- Heavy workspace code must be code-split from public routes.

## Do

- keep content pages simple
- server-render text-heavy pages
- isolate heavy frontend code to workspace pages
- code-split large app routes
- use progressive enhancement for simple forms
- optimize for maintainability and runtime efficiency

## Do Not

- do not turn the whole site into a SPA by reflex
- do not hydrate marketing pages just for stack uniformity
- do not run heavy business logic in the browser by default
- do not use editor-grade architecture for basic dashboard pages
- do not add a global state layer unless the need is real

## Final Instruction

Choose the lightest frontend architecture that fits the page's real interaction needs.

Short rule:

`Static/server-rendered for content, moderate client code for app screens, rich SPA only for true workspaces, heavy computation backend-first.`
