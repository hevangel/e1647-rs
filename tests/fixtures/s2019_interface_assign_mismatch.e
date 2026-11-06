<'
interface shape { area(): uint; };
struct blob { };
extend sys {
    run() is also {
        var s: shape = new blob;
    };
};
'>
