#define FLAG
struct s {
    #ifdef FLAG {
        flagged: bool;
    } #else {
        this is not e at all !!!
    };
};
