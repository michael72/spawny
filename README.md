# spawny
A simple tool to spawn and manage multiple programs in parallel sequences. 

The first argument to spawny is the actual separator.

Examples:
```sh
# opens both editors and exits if one of the editors is closed
spawny :: gedit :: meld

# executes hello and world (the latter with parameter --doit) in parallel
spawny -:- hello -:- world --doit
```

The actual separator doubled means that the following command will be executed sequentially when the previous command finishes.

Example:
```sh
# delayed execution of the client after the server started
# the server is killed when the client or the other way round.
spawny :: server --some-param --another-param=x \
   :: sleep 2 :::: client -param
```

When a chain of commands (or the single command) executed in parallel finishes, the whole
program is finished and all remaining process chains terminated. 
Also wenn spawny gets the signals `SIGTERM` or `SIGINT` (keyboard interrupt) it sends a `SIGTERM`to the running processes. 
          

The separator could be any character except special characters (inside a shell).
See https://mywiki.wooledge.org/BashGuide/SpecialCharacters
