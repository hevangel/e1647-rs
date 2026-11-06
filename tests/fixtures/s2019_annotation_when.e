<'
// IEEE 1647-2019 clause 35.3: an annotation cannot be attached to a
// when block (E0140). The annotation type is not declared here — the
// attachment alone is the offence under test.
struct packet_s { kind: [ imm, reg ];
    @development when imm'kind { };
};
'>
