.PHONY: sqlx
sqlx:
	rm -rf target/sqlx
	env -u DATABASE_URL SQLX_OFFLINE=false cargo check --workspace
	$(eval DB=$(shell jq .db sqlx-data.json))
	jq --compact-output -s '{"db": $(DB)} + (INDEX(.hash)|to_entries|sort_by(.key)|from_entries)' target/sqlx/query-*.json > sqlx-data.json.tmp || ( rm sqlx-data.json.tmp && exit 1 )
	mv -f sqlx-data.json.tmp sqlx-data.json
