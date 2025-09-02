#!/bin/bash
set -euo pipefail

function find_next_number {
  find "$current_dir/../src/database/migrations" -type f -exec basename {} \; |
    cut -d "-" -f 1 |
    sort -rn |
    head -n 1 |
    sed 's/$/ + 1/' |
    bc
}

current_dir="$( cd "$(dirname "${BASH_SOURCE[0]}")" ; pwd -P )"
migrations_directory="$( cd "$(dirname $current_dir/../src/database/migrations)"; pwd -P )/migrations"

read -p "Enter the name of the new migration: " -r
name=$REPLY

next_number="$(find_next_number)"
migration_filename="$next_number-$name.sql"
migration_path="$migrations_directory/$migration_filename"

echo "Save $migration_path?"
read -p "Any key to accept, n to cancel: " -n 1 -r
echo

if [[ $REPLY =~ ^[nN]$ ]]; then
  echo "Migration aborted"
  exit 0
fi

touch "$migration_path"
echo "Saved"
