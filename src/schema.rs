table! {
    message (id) {
        id -> Int4,
        channel -> Text,
        nick -> Nullable<Text>,
        line -> Nullable<Text>,
        opcode -> Nullable<Text>,
        oper_nick -> Nullable<Text>,
        payload -> Nullable<Text>,
        timestamp -> Timestamptz,
    }
}
