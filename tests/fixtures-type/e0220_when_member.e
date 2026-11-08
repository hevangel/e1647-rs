type colors: [red, green];
struct lamp_s {
    kind: colors;
    when red'kind {
        level: uint;
        show() is {
            level = "bright";
        };
    };
};
