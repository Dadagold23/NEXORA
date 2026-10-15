# PROJECT_NAME

A NXL web application, served by the NXL development server.

## Run it

```bash
nexora dev
```

Then open <http://127.0.0.1:3000>. Edit any `.nxl` file and the page reloads
itself; there is no build step to run by hand.

## What is here

```
nxl.toml              manifest; the entry point is src/main.nxl
src/main.nxl          boot sequence; prints the log the page is built from
src/app/config.nxl    name, tagline and features
src/app/routes.nxl    the routes the dev server exposes
src/app/state.nxl     AppState, the boot state machine
public/index.nxl      the page, written in NXL
public/styles.css     styling for the page and the splash screen
public/bootstrap.js   turns the program's print() output into the page
public/favicon.svg    the mark used by the page and the tab
tests/basic.nxl       checks run by `nexora test`
```

## The page is NXL

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

The page is assembled with string concatenation, which is honest about where
NXL is today: there is no template or string-builder type yet.

## Checks

```bash
nexora test      # compiles and runs tests/basic.nxl
nexora build     # JavaScript bundle in build/
```

From the NEXORA repository, `node scripts/verify-web-page.js .` checks that the
page really is wired to the program without needing a browser.