# Web applications

A NXL web application is a set of routes and a set of pages. `nexora serve`
builds a real HTTP server from them; `nexora dev` is the development server for
the browser program.

## Declaring routes

Routes live in `nxl.toml`. Nothing is inferred — what you write is what is
served.

```toml
[web]
host = "127.0.0.1"
port = 8080
# Wrapped around every page. Remove to serve bare pages.
layout = "main.nxlhtml"
# Served, with a real 404 status, for any path that matches no route.
not_found = "404.nxlhtml"

[[route]]
method = "GET"
path = "/"
page = "home.nxlhtml"

[[route]]
# `:id` becomes a variable named `id` in the page.
method = "GET"
path = "/posts/:id"
page = "post.nxlhtml"

[[route]]
method = "GET"
path = "/contact"
page = "contact.nxlhtml"

[[route]]
# Fields listed here must be present and non-empty. A submission missing one
# comes back as 422 with the page redrawn and `error` bound.
method = "POST"
path = "/contact"
page = "contact.nxlhtml"
validate = ["name", "email", "message"]
```

`page`, `layout` and `not_found` are file names inside `pages/` and `layouts/`.
An unknown key in a `[[route]]`, a path that does not start with `/`, a method
other than `GET` or `POST`, and a route with no `page` are all reported with a
line number rather than ignored.

## Pages

Pages are `.nxlhtml` templates. Two forms, both of which leave the document
valid markup:

- `<nxl> … </nxl>` — statements, run where they appear. What they print joins
  the page.
- `<nxl expr="…" />` — one value, **escaped on output, always**.
- `<nxl raw="…" />` — one value, inserted as markup. This is how a layout
  inserts a page it already rendered; it is the deliberate way to opt out of
  escaping, so it is spelled differently.

## Values a page can use

Path parameters, query fields and submitted form fields are bound as variables
before the page runs, so `id` is just `id`:

```html
<h1>Post <nxl expr="id" /></h1>
```

NXL has no way to ask whether a name exists, so these are always bound, to an
empty string when the request did not supply them:

| Name | Meaning |
| --- | --- |
| `method` | `"GET"` or `"POST"` |
| `path` | The request path |
| `error` | The validation message, or `""` |
| `has_error` | `"true"` or `"false"` |
| `q` | Conventional search-term query parameter |
| `page` | Conventional page-number query parameter |

A field listed in `validate` is also always bound on a POST, so a page that
draws a form still compiles on the submission that is missing something.

## Forms

```html
<form method="post" action="/contact">
  <input name="name" value="<nxl expr='name' />">
  <button type="submit">Send</button>
</form>
```

The page is redrawn with what was submitted, and `error` carries the message.
Nothing is stored: there is no session handling and no database behind this.

## What this is not

NXL has no HTTP types, so a page is a template, not a handler. It cannot choose
a status code, set a header, or call another route. Adding any of those needs
request and response types in the language first.

Pages render by printing: every printed piece becomes its own line, exactly as
a plain `.nxl` program does. The newlines are harmless in HTML but mean byte
comparisons against a page need to account for them.

NXL arrays cannot be appended to yet — `+`, `push` and `len` are all absent —
so a page that would collect results prints them as it finds them instead.