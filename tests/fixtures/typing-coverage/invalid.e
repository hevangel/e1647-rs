struct plain_s {
    value: uint;
};
struct covered_s {
    ev: event;
    obj: plain_s;
    label: string;
    count: uint;
    cover ev is {
        item wrong: uint = "text";
        item missing: uint = obj.absent;
        item indexed: uint = count[0];
        item arithmetic: uint = label * 2;
        item label: uint;
    };
};
