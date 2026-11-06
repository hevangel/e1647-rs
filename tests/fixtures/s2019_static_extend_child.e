<'
struct base {
    static g(): uint is {
        return 1;
    };
};
struct child like base {
};
extend child {
    static g() is also {
    };
};
'>
