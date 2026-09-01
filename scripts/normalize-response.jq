def normalize_response($ignored; $ignored_recursive):
    reduce $ignored[] as $key (. ; del(.[$key]))
    | walk(
        if type == "object" then
            with_entries(
                .key as $key
                | select(($ignored_recursive | index($key)) == null)
            )
        else
            .
        end
    );

normalize_response($ignored; $ignored_recursive)
