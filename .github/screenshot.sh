#!/bin/bash

set -euxo pipefail

declare -r cwd="$(dirname ${BASH_SOURCE[0]})/.."
declare -r chrome="${CHROME:-chromium}"
declare -r httpport=9220
declare -r dbport=9221
declare -r url="http://127.0.0.1:$httpport/libera:~h~dolphin-dev/2021-09-01"
declare -r db="postgresql://postgres@127.0.0.1:$dbport/db"

declare -r container=$(docker run --rm --detach \
    -e POSTGRES_HOST_AUTH_METHOD=trust -p "$dbport":5432 -e POSTGRES_DB=db \
    postgres:12-alpine postgres)

while ! docker exec "$container" psql --db db --user postgres -c 'SELECT 1'; do sleep 1; done

# Run with no paths, to migrate. Returns non-zero, so bypass failure.
( cd "$cwd" && IRCJ_DB="$db" cargo run --bin ircj-watch || true )

# Insert dummy data.
docker exec -i "$container" psql --db db --user postgres < "$cwd/testlogs/dummy.sql"

# Run the frontend.
( cd "$cwd" && cargo build --bin ircj-serve )
ROCKET_PORT="$httpport" ROCKET_ADDRESS=127.0.0.1 IRCJ_DB="$db" "$cwd/target/debug/ircj-serve" &
declare -r pid=$!
while ! curl --silent -o /dev/null "$url"; do sleep 1; done

# Screenshot and add shadow.
pushd "$cwd/.github"
"$chrome" --headless --disable-gpu --screenshot --hide-scrollbars --window-size=900,584 "$url"
mv {,new}screenshot.png
convert newscreenshot.png \( +clone -background black -shadow 80x4+0+0 \) +swap -background white -layers merge +repage screenshot.png
optipng -quiet -clobber screenshot.png
rm -f newscreenshot.png
popd

docker stop --time=2 "$container" &
kill -TERM "$pid" &

wait
