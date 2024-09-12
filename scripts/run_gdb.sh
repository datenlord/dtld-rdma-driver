#!/bin/env bash

gdb -x ./scripts/gdb_init_cmd.txt -ex "target remote :1234"

