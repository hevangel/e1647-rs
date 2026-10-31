struct base_struct {
    id: int;
};
template struct iterator of <type> like base_struct {
    idx: int;
};
template struct deque_iterator of <type> like iterator of <type> {
    pos: int;
};
template struct container of <type> like base_struct {
    get_iterator(): iterator of <type> is undefined;
    add(item: <type>) is undefined;
};

package template struct deque of <type> like container of <type> {
    private items: list of <type>;
    front: uint;

    set(index: uint, item: <type>) is {
        items[index] = item;
    };

    get_iterator(): iterator of <type> is {
        result = new deque_iterator of <type> with {
            it.deque = me;
        };
    };
};

template struct map of (<key'type>, <value'type> = <key'type>) {
    put(k: <key'type>, v: <value'type>) is {
        var size := 2;
        size += 1;
    };
    get_all(): list of <key'type> is undefined;
};

type small_int : int (bits : 8);
