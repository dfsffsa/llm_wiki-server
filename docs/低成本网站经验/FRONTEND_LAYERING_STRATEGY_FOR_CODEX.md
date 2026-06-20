# Frontend Layering Strategy For Indie Developers

Purpose:

- This document is meant for Codex or another coding LLM.
- It defines how to structure the frontend of a small, bootstrapped product so it stays fast, cheap to run, and easy to maintain.
- It is inspired by the observed patterns across Steve Hanov's sites:
  - static/server-rendered content pages
  - narrowly scoped interactive SPA workspaces
  - heavy computation moved to the backend

This is not a generic enterprise frontend architecture.
It is a practical strategy for solo founders or very small teams.

---

## Core Principle

Do not choose one frontend architecture for the whole product.

Instead:

1. Keep content and marketing pages as simple as possible.
2. Use a SPA only for pages that truly need rich client-side interaction.
3. Move heavy logic, expensive computation, scoring, AI, rendering, and large data processing to the backend.

The goal is not "minimal JavaScript at all costs".
The goal is "put complexity only where it creates product value".

---

## Product Surface Classification

Every page should be placed into one of these layers.

### Layer A: Content Pages

Examples:

- landing page
- pricing page
- blog
- docs
- legal pages
- simple FAQ
- changelog

Default implementation:

- server-rendered HTML or static HTML
- minimal CSS
- little or no client-side JavaScript
- no SPA shell
- no client-side hydration unless there is a very specific reason

Rules:

- prioritize fast first paint
- prioritize SEO
- prioritize low memory usage
- prefer link navigation over client-side router complexity

Codex instruction:

If building a Layer A page, do not default to a React/Vue SPA unless explicitly required by the surrounding codebase.
Prefer static markup, progressively enhanced forms, and small scripts.

### Layer B: Interactive Workspace Pages

Examples:

- code-like editor
- diagram builder
- dashboard builder
- content editor
- visual design tool
- drag-and-drop UI
- anything with persistent local UI state

Default implementation:

- isolated SPA or isolated client-heavy route
- framework is acceptable
- local UI state is acceptable
- component complexity is acceptable
- only for a narrow section of the product

Rules:

- keep the SPA boundary local to this workspace
- do not force the entire site into the same SPA shell
- do not make the browser do heavy business computation unless unavoidable

Codex instruction:

If building a Layer B page, optimize for interaction quality inside the page.
Do not expand the same client-heavy model to unrelated marketing or content pages.

### Layer C: Authenticated App Screens

Examples:

- account dashboard
- billing screen
- saved items list
- personal workspace list
- reports page
- subscriber-only data views

Default implementation:

- can be server-rendered with islands
- or can be a moderate SPA section
- should remain simpler than a full creative workspace

Rules:

- do not move large analytics pipelines or AI processing into the browser
- fetch processed results from the backend
- use client state only for interaction, filters, tabs, and local UX

Codex instruction:

If building a Layer C page, keep client logic moderate.
Prefer server-driven data and focused interactivity over framework-heavy abstraction.

---

## Architecture Rules

## Rule 1: Do Not Use One Global SPA By Default

Bad default:

- every page shares one heavy application shell
- marketing, docs, auth, and editors all hydrate the same runtime

Preferred default:

- Layer A is static or server-rendered
- Layer B is client-heavy only where needed
- Layer C is moderate and data-driven

This reduces:

- initial JS cost
- browser memory usage
- routing complexity
- debugging complexity

## Rule 2: Push Heavy Work To The Backend

The browser should not be the main place for:

- AI inference
- large ranking/scoring pipelines
- document parsing
- heavy image generation
- diagram layout algorithms if a backend render path is acceptable
- large dataset joins or aggregations

Preferred pattern:

- frontend sends intent or input
- backend computes
- frontend renders result

This is often the single highest-impact performance decision.

## Rule 3: Keep Rich Frontends Narrow

If one page needs a code editor, canvas, or complex visual state, that is fine.
But the rich frontend should be limited to that page or route group.

Do not let a single complex workspace architecture dictate the rest of the site.

## Rule 4: Favor Progressive Enhancement On Simple Screens

For forms like:

- sign in
- register
- forgot password
- contact
- comments

prefer:

- simple HTML forms
- fetch enhancement only if useful
- small validation scripts

Do not build a large client state model for trivial forms.

## Rule 5: Treat SEO and Shareability As First-Class Constraints

For public pages:

- content should exist in HTML
- metadata should exist in HTML
- avoid requiring client-side rendering for primary text

This matters more for indie products than framework purity.

---

## Default Technology Recommendations

These are defaults, not hard requirements.

### For Layer A

Preferred:

- plain server-rendered templates
- static site generator
- minimal enhancement scripts

Avoid:

- full SPA
- heavy hydration
- complex client stores

### For Layer B

Preferred:

- React or Vue only if the interaction actually needs it
- route-local code splitting
- editor/canvas libraries only where necessary
- backend-assisted rendering and saving

Avoid:

- putting the entire product in the same app shell
- large global state for unrelated pages

### For Layer C

Preferred:

- server-fetched data
- small client components for filters, tabs, modals, inline actions
- SSR or hybrid rendering when possible

Avoid:

- rebuilding all backend business logic in the browser

---

## Performance Budget Guidance

These are pragmatic targets, not absolute laws.

### Layer A Target

- keep page JS close to zero if possible
- if JS exists, it should be small and optional
- prioritize fast first contentful paint

### Layer B Target

- accept a larger bundle if it directly powers the product's core interaction
- compensate by:
  - scoping it to a narrow route
  - splitting code aggressively
  - offloading heavy processing to the server

### Layer C Target

- keep bundle size moderate
- avoid dragging workspace dependencies into dashboard routes

---

## State Management Rules

### Use No Global Store Unless There Is A Real Cross-Page Need

Good reasons:

- current authenticated user
- feature flags
- billing/subscription summary
- very small global UI preferences

Bad reasons:

- storing every form field
- storing every list item globally
- replicating backend state for convenience

### Prefer Local State First

For most screens:

- local component state
- route-level fetched data
- server as source of truth

Only introduce a global store when duplication or cross-route coordination becomes real.

---

## Routing Rules

### Public Routes

- prefer normal document navigation or server routing
- keep them crawlable and simple

### App Routes

- client-side routing is acceptable
- but only within the authenticated app surface

### Workspace Routes

- code split aggressively
- do not preload the editor/canvas system for unrelated pages

---

## Data Fetching Rules

### For Public Pages

- fetch on the server where possible
- avoid client waterfalls

### For Authenticated Screens

- return already-shaped data from the backend
- avoid client-side stitching of multiple expensive endpoints

### For Workspaces

- use incremental saves
- debounce expensive actions
- keep result polling or preview regeneration controlled

---

## UI Complexity Rules

### Marketing Pages

- no giant app shell
- no unnecessary animation runtime
- no component system overhead beyond what is needed

### Editors / Builders

- interaction quality matters
- allow more JavaScript here
- but confine it to this route

### Dashboards

- keep charts and tables lazy-loaded
- avoid loading all panels up front

---

## Codex Implementation Policy

Use these rules when writing code.

1. Before adding a frontend framework to a page, classify the page into Layer A, B, or C.
2. For Layer A, default to server-rendered or static output.
3. For Layer B, allow a framework, but keep the SPA boundary local.
4. For Layer C, keep client interactivity focused and backend-driven.
5. Do not put heavy business logic in the browser if the backend can do it more reliably.
6. Do not create a single global frontend architecture unless the product genuinely requires it.
7. Prefer simpler deployment and lower runtime memory over fashionable frontend patterns.

---

## Explicit Do / Do Not Instructions

### Do

- separate public pages from app/workspace pages
- keep content pages simple
- server-render text-heavy pages
- use client-heavy code only for real interactive workspaces
- push computation to the backend
- use code splitting for heavy routes
- keep authentication screens lightweight

### Do Not

- turn the whole site into a SPA by reflex
- hydrate marketing pages just to keep the stack uniform
- run ranking, AI, document parsing, or large render jobs in the browser by default
- couple dashboards to editor-grade client architecture
- create a large global state tree for simple flows

---

## Example Product Mapping

Use this as a mental template.

### Example: SaaS Tool Site

- `/` -> Layer A
- `/pricing` -> Layer A
- `/blog/*` -> Layer A
- `/login` -> Layer A or very light Layer C
- `/register` -> Layer A or very light Layer C
- `/app` -> Layer C
- `/app/settings` -> Layer C
- `/app/billing` -> Layer C
- `/editor/:id` -> Layer B

### Example: AI Data Product

- landing page -> Layer A
- reports list -> Layer C
- account page -> Layer C
- query builder -> Layer B
- AI scoring job -> backend
- exported report generation -> backend

---

## Final Instruction To Codex

When implementing frontend code for an indie product:

- choose the lightest architecture that still fits the page's real interaction needs
- keep public content pages simple
- keep heavy frontend logic local to the specific workspace that needs it
- move expensive logic to the backend
- optimize for maintainability and runtime efficiency, not architectural uniformity

Short version:

`Static or server-rendered for content, moderate client code for app screens, rich SPA only for true workspaces, heavy computation always backend-first.`
