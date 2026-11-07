<'
// IEEE 1647-2019 clause 35.2: an annotation type cannot be extended —
// its members are fixed at declaration (E0320).
annotation @development { developer: string; date: string; release: uint; };
extend development { };
'>
