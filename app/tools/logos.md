# Service logos

`assets/logos/*.svg` come from [Simple Icons](https://simpleicons.org) 16.33.0 (CC0-1.0), unchanged,
one file per slug. The logos are trademarks of their owners; they only label the shortcut that opens
that service.

The remote draws them in ink like every other key (`lib/screens/remote.dart`, `logoFor`). A
`[[agent.shortcut]]` names its logo with `icon = "<slug>"`; apps without one get a logo when their
name matches a slug or an alias, otherwise their initials.

`assets/shortcuts.json` is the catalogue of sites the phone can add to the remote (name, logo,
address, player keys): data, so the code knows no service. A new entry needs its logo here.

To add one: download `https://cdn.jsdelivr.net/npm/simple-icons@<version>/icons/<slug>.svg` into
`assets/logos/`, then add the slug to `_logos` in `remote.dart`.
