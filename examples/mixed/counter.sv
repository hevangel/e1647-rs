`timescale 1ns/1ns
// Mixed e + SystemVerilog testcase DUT: a free-running 8-bit counter
// driven by e1647-rs through the xezim VPI bridge (see
// examples/mixed_counter.e).
module counter (
    input  wire       clk,
    input  wire       rst,
    output reg  [7:0] count
);

    always @(posedge clk or posedge rst) begin
        if (rst) begin
            count <= 8'd0;
        end else begin
            count <= count + 8'd1;
        end
    end

endmodule
