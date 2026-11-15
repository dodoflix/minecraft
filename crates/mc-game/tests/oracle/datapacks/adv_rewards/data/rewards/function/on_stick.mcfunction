say rewarded
setblock ~ ~-1 ~ minecraft:gold_block
scoreboard players add @s marks 1
execute if score @s marks matches 1 run give @s minecraft:apple 3
