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
public/index.html     the landing page, including the splash screen
public/styles.css     styling for the page and the splash screen
public/bootstrap.js   turns the program's print() output into the page
public/favicon.svg    the mark used by the page and the tab
tests/basic.nxl       checks run by `nexora test`
```

## How the page gets its content

NXL has no DOM types yet, so the program does not write to the page directly:
`print()` compiles to `console.log`, and `public/bootstrap.js` takes that over
before `/app.js` runs. Lines the program prints become page content:

| Program prints        | Page effect                    |
| --------------------- | ------------------------------ |
| `booting <name>`      | hero heading and document title |
| `tagline <text>`      | hero sub-heading               |
| `route  <path>`       | served-routes list             |
| `feature  <text>`     | feature list                   |
| `READY`               | clears the splash screen       |

That last one matters: the splash screen is cleared when the program says it
is ready, not after a timer. If the program throws, the splash stays up and the
reason is printed instead of the app pretending to be healthy.

## Checks

```bash
nexora test      # compiles and runs tests/basic.nxl
nexora build     # JavaScript bundle in build/
```