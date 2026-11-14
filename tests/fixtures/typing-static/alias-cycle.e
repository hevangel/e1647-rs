type recursive_t: recursive_t;
struct api_s { static value: recursive_t; };
extend sys { run() is also { }; };
