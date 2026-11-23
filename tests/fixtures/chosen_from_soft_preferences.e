struct option_s {
    code: uint;
};
struct chooser_s {
    !choices: list of option_s;
    !source_reads: uint = 0;
    target: option_s;
    collect_choices(): list of option_s is {
        source_reads = source_reads + 1;
        return choices;
    };
    keep target chosen_from read_only(collect_choices());
    keep target.code >= 3;
    keep soft target.code == 5;
    generate_target(): uint is {
        gen target;
        return target.code;
    };
};
extend sys {
    !chooser: chooser_s;
    run() is also {
        chooser.target.code = 42;
        for each (code) in {2; 3; 5} {
            var candidate: option_s = new;
            candidate.code = code;
            chooser.choices.push(candidate);
        };
        gen chooser.target;
        print chooser.target.code, chooser.target == chooser.choices[2];
        chooser.choices.delete(2);
        print chooser.generate_target(), chooser.target == chooser.choices[1];
        print chooser.source_reads;
    };
};
