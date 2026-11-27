CI release jobs no longer emit empty `--remap-path-prefix` FROM values (which corrupted relative paths) and now fail the build if the binary still contains build-machine paths.
