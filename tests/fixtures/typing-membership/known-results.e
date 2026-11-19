struct api_s {
    nums(): list of uint is { return { 1; }; };
    static words(): list of string is { return { "word"; }; };
};
extend sys {
    api: api_s;
    run() is also {
        var a := api.nums()["bad"];
        var b := "bad" in api.nums();
        var c := api.nums() in api_s::words();
    };
};
