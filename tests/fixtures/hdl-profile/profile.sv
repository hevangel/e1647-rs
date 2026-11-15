`timescale 1ps/1ps
// Independently authored bridge profile. The runner stages the wave path.
module profile;
    reg signed [31:0] word;
    reg [63:0] wide;
    reg [3:0] unknown_bits;
    reg clk;
    reg [7:0] count;
    initial begin
        word = 0;
        wide = 0;
        unknown_bits = 4'bxz01;
        clk = 0;
        count = 0;
        $dumpfile("PROFILE_WAVE");
        $dumpvars(0, profile);
    end
    always @(posedge clk) count <= count + 1;
endmodule
