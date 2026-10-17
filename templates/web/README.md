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

## What is here

```
nxl.toml              manifest; routes, layout and 404 page
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
holding the message.

See `docs/20-web-applications.md` in the NEXORA repository for the full
reference, including the limits — short version: NXL has no HTTP types yet, so
a page is a template and not a handler.

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

## Checks

```bash
nexora test      # compiles and runs tests/basic.nxl
nexora build     # JavaScript bundle in build/
```

From the NEXORA repository, `node scripts/verify-web-page.js .` checks that the
page really is wired to the program without needing a browser.