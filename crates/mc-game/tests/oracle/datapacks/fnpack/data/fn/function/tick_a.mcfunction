scoreboard players add #tick fn 1
execute if score #tick fn matches ..3 run data modify storage fn:state ticks append value "a"
