# PROJECT_NAME

A NXL web application, served by the NXL development server.

## Run it

```bash
nexora serve     # the routes declared in nxl.toml
nexora dev       # the browser program, with live reload
```

`serve` builds a real HTTP server from the `[[route]]` table in `nxl.toml` and
listens on `127.0.0.1:8080`. `dev` serves `public/index.nxl`, compiles `src/` to
JavaScript and reloads the page on <http://127.0.0.1:3000> when you edit a file.

## Make it yours

| What | Where |
| --- | --- |
| Application name (nav, `<title>`, hero) | `name` in `nxl.toml` — the layout binds it as `app_name` |
| Bind address and port | `[web]` `host`/`port` in `nxl.toml`, or `[env.production]` for a deployment |
| Pages | one `.nxlhtml` file in `pages/`, referenced by a `[[route]]` entry |
| Navigation | `layouts/main.nxlhtml` (top nav and footer) |
| Feature cards, hero copy | `pages/home.nxlhtml` |
| Form fields | `pages/contact.nxlhtml` plus the route's `validate` list |
| Colors, spacing, buttons | the custom properties at the top of `public/styles.css` |

## Adding a page

1. Create `pages/pricing.nxlhtml` — plain markup, with `<nxl expr="…" />` where a
   value should appear (escaped) and `<nxl> … </nxl>` for statements.
2. Declare it in `nxl.toml`:

   ```toml
   [[route]]
   method = "GET"
   path = "/pricing"
   page = "pricing.nxlhtml"
   ```

3. Add a link in `layouts/main.nxlhtml`.
4. Run `nexora serve` and open <http://127.0.0.1:8080/pricing>.

A route whose page is missing, or with an unknown key, is refused at startup
with the line number in `nxl.toml` — the server never serves a half-declared
route.

## What is here

```
nxl.toml              manifest; routes, layout, 404 page, project name
pages/                one .nxlhtml template per route
layouts/main.nxlhtml  wrapped around every page
public/               static assets, plus the dev-server landing page
src/main.nxl          boot sequence; prints the log the landing page uses
src/app/config.nxl    name, tagline and features
src/app/routes.nxl    the routes the dev server exposes
src/app/state.nxl     AppState, the boot state machine
tests/basic.nxl       checks run by `nexora test`
```

## Pages

Pages are templates. `<nxl> … </nxl>` runs statements where you put them, and
`<nxl expr="…" />` prints one value **escaped**, always. Path parameters and
submitted form fields are bound as variables before the page runs, so a
`/posts/:id` route can write:

```html
<h1>Post <nxl expr="id" /></h1>
```

A form posting to a route with `validate = ["name", "email"]` comes back as
`422` with the page redrawn, the submitted values still in place, and `error`
holding the message. A submission that passes validation renders the same page
with `method` set to `POST`, which is how `pages/contact.nxlhtml` shows its
success state.

### What serving a page cannot do yet

This is the honest boundary of the current toolchain, not a stylistic choice:

- **No request/response types.** NXL has no HTTP types, so a page is a template
  and not a handler: it can render, but it cannot read headers, set status
  codes per case or return JSON. `q`, `method`, `path`, the path parameters and
  the submitted fields are the whole binding surface.
- **No state between requests.** Nothing is stored; a submission is validated
  and echoed, then forgotten. There is no database behind this starter.
- **`serve` and `dev` are separate.** `serve` renders the routes in
  `nxl.toml`; `dev` runs the browser program in `src/`. They listen on
  different ports (8080 and 3000) and do not share a request pipeline.
- **Arithmetic edge cases.** A crafted path can make an expression unusable
  where the language has no value for the result yet — for example
  `/posts/1+2` is handed to the template verbatim.

Full reference, including these limits: `docs/20-web-applications.md` in the
NEXORA repository.

## The dev-server landing page

`public/index.nxl` is a program. `nexora dev` compiles it, runs it, and serves
what it prints as HTML — so the page is built from the same modules as the app
instead of being markup somebody has to keep in step by hand:

```
from app.config import app_name;
```

Change `app_name()` and both the program and the page show the new name.

You can also run it directly:

```bash
nexora run public/index.nxl
```

The page is the shell. In the browser, `print()` compiles to `console.log` and
`public/bootstrap.js` takes that over before the program runs, so the boot log
keeps the page live:

| Program prints   | Page effect                        |
| ---------------- | ---------------------------------- |
| `booting <name>` | hero heading and document title    |
| `tagline <text>` | hero sub-heading                   |
| `route  <path>`  | served-routes list                 |
| `feature  <text>`| feature list                       |
| `READY`          | clears the splash screen           |

That last one matters: the splash screen is cleared when the program says it is
ready, not after a timer. If the program throws, the splash stays up and the
reason is printed instead of the app pretending to be healthy.

The landing page is assembled with string concatenation, which is honest about
where NXL is today: it has no template type, so `pages/` uses the `.nxlhtml`
template format instead.

## Security and production

Every response carries `x-content-type-options: nosniff`,
`referrer-policy: strict-origin-when-cross-origin` and
`x-frame-options: DENY`. Output through `<nxl expr>` is escaped, and route
tables and manifests are validated at startup with real error codes and a
non-zero exit.

`serve` is a **development** server for the pages in `pages/`. It is not
intended to be exposed directly to the internet: it has no TLS, no rate
limiting and no authentication. Deployment and environment configuration
(`NXL_ENV`, `[env.<name>]`, required variables) are covered by
`docs/21-deployment.md` in the NEXORA repository.

## Checks

```bash
nexora test      # compiles and runs tests/basic.nxl
nexora build     # JavaScript bundle in build/
```

From the NEXORA repository, `node scripts/verify-web-page.js .` checks that the
page really is wired to the program without needing a browser.
