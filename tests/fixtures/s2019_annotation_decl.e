<'
// IEEE 1647-2019 clause 35.2: the annotation type declaration. A
// struct-like type in the type namespace; `repeatable` licenses more
// than one attachment of the type on one entity (clause 35.3).
annotation @development { developer: string; date: string; release: uint; };
repeatable annotation @reviewed { by: string; };
'>
