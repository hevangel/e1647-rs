interface simple_i { f(x: int); };
interface inherited_i like simple_i { };
struct base_s { f(x: string) is { }; };
struct child_s like base_s implementing inherited_i { };
extend added_s implementing simple_i { };
struct added_s { f(x: string) is { }; };
struct extension_s implementing simple_i { f(x: string) is { }; };
extend extension_s { f(x) is also { }; };
