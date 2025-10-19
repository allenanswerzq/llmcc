[ ] make the descriptors as standalone cargo project, do some abstraction for all languages
[ ] incremental parsing

please help me implement the llmcc-toml crate, it should serve the same purpose as llmcc-rust, where it parses any .toml file, then evently build graph relations, for
a rust repo, we can use this info to build module graphs..

the utlimae goal is build
a multi level index engine for any repo. something like:
repo -> folder graph(which folder of stuff maybe related) -> each subfoler graph -> code graph, given a big repo, of course we dont want to build it at once, we probably going to do lazy build graph or something.

after we have this, give some code, like a name or function or a file or a folder, we could find all related stuff with it in the whole repo, compare to command tools like rg
we can find correct context in one turn, but building graphs can take time, rg is simple but needs multiple truns to fully understand context... writing this text made me wundering is it even warth doing this project...



