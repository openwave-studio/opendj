# opendj.app

The openDJ website: a static landing page, a tutorials page, release pages
and the docs, which are a [Starlight](https://starlight.astro.build) site in
`docs/` (Node 22 or newer).

## Preview

```sh
./build.sh                       # builds everything into dist/
python3 -m http.server -d dist   # then open http://localhost:8000
```

For the docs alone, `cd docs && npm install && npm run dev` reloads as you
edit (without the site's fonts, which live in `assets/`).

## Docs

Pages are Markdown files in `docs/src/content/docs/`, served at
`/docs/<path>.html` (the addresses the app's tutorials link to). Each starts
with a `title:` front matter instead of a `#` heading. Link to other pages by
file, `[Looping](looping.md#loop-rolls)`: `docs/plugins/md-links.mjs` turns
that into the page's address, and the build fails on a link to a page or
heading that doesn't exist. Screenshots go in `docs/src/content/docs/images/`
and are linked relatively; the build converts them to WebP. The sidebar is
listed in `docs/astro.config.mjs`, so add new pages there. The look (Electric
Lilac, Inter Tabular, Zen Dots) is in `docs/src/styles/opendj.css`, and
`docs/src/components/SiteTitle.astro` puts the openDJ logo, linking home, in
the header. Old addresses that moved are listed under `redirects` in the
config.

## Deploy (Cloudflare Pages, free plan)

Connect the repository in Cloudflare Pages with:

| Setting | Value |
|---|---|
| Production branch | `main` |
| Framework preset | None |
| Root directory | `website` |
| Build command | `./build.sh` |
| Build output directory | `dist` |

`build.sh` installs the docs' packages with `npm ci` and builds them; the
build image picks Node from `.node-version`. Add
`opendj.app` under the project's Custom domains. `_headers` sets caching and
security headers, and `404.html` is the not-found page.

The Download buttons link to the latest GitHub release; `main.js` swaps in the
direct file links from GitHub's API when it can. Below them, the Nightly lane
reads the `nightly` pre-release (named after the next version, shown as
"0.9.0 Alpha") and Previous versions lists the older stable releases.

## Release pages

`releases/` holds a page per version for people who use openDJ (what's new,
one section per area with a screenshot), and `releases/index.html` lists them.
The technical changelog stays on GitHub. For a new version, copy the last
page to `releases/<major.minor>/index.html`, put its screenshots (WebP) in
`assets/releases/<major.minor>/`, write a short section per area from the
GitHub release notes, and add it at the top of `releases/index.html`.

## Mixcloud sign-in

`functions/mixcloud/` holds two Cloudflare Pages Functions that let the app
sign DJs in to their own Mixcloud accounts without shipping the app's client
secret: `/mixcloud/login` sends the DJ to Mixcloud, `/mixcloud/callback`
swaps the code for a token and hands it to the app on `127.0.0.1`. They need
two variables in the Pages project (Settings › Variables and Secrets, for
Production):

| Variable | Value |
|---|---|
| `MIXCLOUD_CLIENT_ID` | the client id of the openDJ app on mixcloud.com/developers |
| `MIXCLOUD_CLIENT_SECRET` | its client secret (type: Secret) |

Register the app at https://www.mixcloud.com/developers/create/ with the
redirect URI `https://opendj.app/mixcloud/callback`. Until the variables are
set, the sign-in pages say it isn't set up yet.

## Controller pages

`tools/controllers.py` writes a docs page for every mapping in
`res/controllers` when the docs build (`npm run build` and `npm run dev` run
it first), plus the controllers' index page and `docs/src/controllers.json`,
their sidebar entries. Hand-written guides in `docs/controller-guides/` go at
the top of their controller's page, and the controllers in
`crates/controllers/support.json` get a Beta or Supported badge. Lines that
drive a control openDJ doesn't have yet are marked using
`docs/supported-controls.txt`. `cargo test` fails when that list is
out of date; regenerate it from the repository root with:

```sh
cargo run -q -p opendj -- --list-controls > website/docs/supported-controls.txt
```
