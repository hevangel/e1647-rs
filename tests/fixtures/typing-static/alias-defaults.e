type num_t: uint;
type chain_t: num_t;
type flag_t: bool;
type words_t: list of uint;
struct payload_s { n: uint; };
type box_t: payload_s;
type mode_t: [hot_mode, cold_mode];
type mode_alias_t: mode_t;
struct api_s {
    static n: chain_t;
    static flag: flag_t;
    static words: words_t;
    static box: box_t;
    static mode: mode_alias_t;
};
extend sys {
    run() is also {
        check that api_s::n == 0;
        check that api_s::flag == FALSE;
        check that api_s::words.size() == 0;
        check that api_s::box.n == 0;
        check that api_s::mode == hot_mode;
        var n: chain_t;
        var flag: flag_t;
        check that n == 0;
        check that flag == FALSE;
        out("alias defaults passed");
    };
};
