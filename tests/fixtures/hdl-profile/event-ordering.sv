`timescale 1ps/1ps
module event_ordering;
    reg clk = 0;
    reg [7:0] count = 0;
    reg [7:0] edge_tick = 0;
    reg [7:0] delayed_count = 0;

    always @(posedge clk) begin
        count <= count + 1;
        edge_tick <= $time;
        delayed_count <= #1 count + 1;
    end
endmodule
