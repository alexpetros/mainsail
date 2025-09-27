> [!CAUTION]
> This software is shared for educational purposes and I do not recommend integrating it at this time.
> It has no stable interfaces and many of the statements in this README are aspirational.

# Mainsail

Mainsail adds drop-in ActivityPub support to a web server.
You don't need to think about the database, the protocol, or handling requests.
You just add the Mainsail layer and you're good to go.

## Why

ActivityPub is a protocol for website-to-website communication, largely for user-generated content.

Existing ActivityPub libraries implement the protocol, but they require you to define your own data structures and persistence layer.
This is a reasonable choice that allows for a lot of flexibility, but it asks a lot of the library user.

Mainsail makes ActivityPub integration easier by supplying useful data structures *and handling persistence*.
This makes it much easier to add ActivityPub support to an existing web service.

Mainsail is able to handle persistence on its own because Mainsail only supports SQLite.
In fact, Mainsail does not expose its persistence layer at all—you provide it with a filepath for its database, and Mainsail takes care of the rest.
If you are also using SQLite for other parts of your application, you can choose to [attach](https://sqlite.org/lang_attach.html) your application's database to the Mainsail one, but this is optional.

This architecture allows Mainsail to support most use-cases with zero additional developer overhead.

## Support

Mainsail exposes a function that creates a layer for the [tower](https://docs.rs/tower/latest/tower/) middleware ecosystem.

It is my hope to eventually support a lot more, including node via WASM.

## When will this be usable?

I'm not sure, but I am actively working on it, and stress-testing it by building different applications on top of it.
If this this interests you, feel free to reach out to me.

## License

Mainsail is licensed under the Apache 2.0 License.
