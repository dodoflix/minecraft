#!/usr/bin/env python3
"""Writes corpus.txt (the input of DialogGolden.java). Vanilla's own dialogs are added from the data pack."""
import json, os, glob

lines = []
def J(v): lines.append("J" + (v if isinstance(v, str) else json.dumps(v, ensure_ascii=True, separators=(",", ":"))))
def N(s): lines.append("N" + s)
def C(s): lines.append("# " + s)

T = {"title": "T"}
def notice(**kw): return {"type": "minecraft:notice", **T, **kw}
def with_common(**kw): return {"type": "minecraft:notice", "title": "T", **kw}
def btn(**kw): return {"label": "L", **kw}

C("vanilla data pack dialogs")
root = os.path.join(os.path.dirname(__file__), "../../../../../ref/src/data/minecraft/dialog")
for f in sorted(glob.glob(root + "/*.json")):
    J(json.dumps(json.load(open(f)), ensure_ascii=True, separators=(",", ":")))
for name in ["server_links", "custom_options", "quick_actions", "minecraft:server_links", "nope", "foo:bar", "Bad Id", ""]:
    J(json.dumps(name))

C("dialog types")
J(notice())
J({"type": "notice", **T})
J({"type": "minecraft:notice", **T, "action": btn(action={"type": "minecraft:run_command", "command": "say hi"}, tooltip="tip", width=77)})
J({"type": "minecraft:confirmation", **T, "yes": btn(), "no": btn()})
J({"type": "minecraft:confirmation", **T, "yes": btn()})
J({"type": "minecraft:multi_action", **T, "actions": [btn()]})
J({"type": "minecraft:multi_action", **T, "actions": []})
J({"type": "minecraft:multi_action", **T})
J({"type": "minecraft:multi_action", **T, "actions": [btn(), btn(width=1024)], "columns": 3, "exit_action": btn()})
J({"type": "minecraft:multi_action", **T, "actions": [btn()], "columns": 0})
J({"type": "minecraft:multi_action", **T, "actions": [btn()], "columns": -1})
J({"type": "minecraft:server_links", **T})
J({"type": "minecraft:server_links", **T, "columns": 1, "button_width": 310, "exit_action": btn()})
J({"type": "minecraft:server_links", **T, "button_width": 0})
J({"type": "minecraft:server_links", **T, "button_width": 1025})
J({"type": "minecraft:dialog_list", **T, "dialogs": "minecraft:server_links"})
J({"type": "minecraft:dialog_list", **T, "dialogs": ["minecraft:server_links", "minecraft:quick_actions"]})
J({"type": "minecraft:dialog_list", **T, "dialogs": "#minecraft:quick_actions"})
J({"type": "minecraft:dialog_list", **T, "dialogs": "#minecraft:pause_screen_additions", "columns": 4, "button_width": 99})
J({"type": "minecraft:dialog_list", **T, "dialogs": "#minecraft:missing"})
J({"type": "minecraft:dialog_list", **T, "dialogs": "minecraft:nope"})
J({"type": "minecraft:dialog_list", **T, "dialogs": ["minecraft:server_links", "minecraft:nope"]})
J({"type": "minecraft:dialog_list", **T, "dialogs": []})
J({"type": "minecraft:dialog_list", **T})
J({"type": "minecraft:dialog_list", **T, "dialogs": [notice()]})
J({"type": "minecraft:dialog_list", **T, "dialogs": notice()})
J({"type": "minecraft:dialog_list", **T, "dialogs": [{"type": "minecraft:multi_action", **T, "actions": []}]})
J({"type": "minecraft:unknown", **T})
J({"type": "minecraft:dialog_list", **T, "dialogs": "minecraft:server_links", "type2": 1})
J({"title": "T"})
J({"type": 5, **T})
J({"type": None, **T})
J({"type": "minecraft:notice"})
J([])
J(5)
J(True)
J(None)
J({})

C("common data")
J(notice(external_title="E"))
J(notice(external_title={"text": "E", "bold": True}))
J(notice(external_title=5))
J({"type": "notice", "title": {"text": "T", "color": "red"}})
J({"type": "notice", "title": ["a", "b"]})
J({"type": "notice", "title": {"bad": 1}})
J({"type": "notice", "title": 5})
J(notice(can_close_with_escape=False))
J(notice(can_close_with_escape="x"))
J(notice(can_close_with_escape=1))
J(notice(pause=False))
J(notice(pause=False, after_action="none"))
J(notice(pause=True, after_action="none"))
J(notice(pause=True, after_action="wait_for_response"))
J(notice(pause=False, after_action="wait_for_response"))
J(notice(pause=False, after_action="close"))
J(notice(after_action="bogus"))
J(notice(after_action=1))
J(notice(pause=1))

C("body")
J(notice(body="plain"))
J(notice(body=["a", "b"]))
J(notice(body=[]))
J(notice(body={"type": "minecraft:plain_message", "contents": "c"}))
J(notice(body={"type": "minecraft:plain_message", "contents": "c", "width": 500}))
J(notice(body={"type": "minecraft:plain_message", "contents": "c", "width": 0}))
J(notice(body={"type": "minecraft:plain_message", "contents": "c", "width": 1025}))
J(notice(body={"type": "minecraft:plain_message"}))
J(notice(body={"type": "minecraft:other"}))
J(notice(body={"contents": "c"}))
J(notice(body=[{"type": "minecraft:plain_message", "contents": "c"}, "x", {"text": "y"}]))
J(notice(body=5))
J(notice(body=[5]))
J(notice(body={"type": "minecraft:item", "item": "minecraft:stone"}))
J(notice(body={"type": "minecraft:item", "item": {"id": "minecraft:stone"}}))
J(notice(body={"type": "minecraft:item", "item": {"id": "minecraft:stone", "count": 5}}))
J(notice(body={"type": "minecraft:item", "item": {"id": "minecraft:stone", "count": 100}}))
J(notice(body={"type": "minecraft:item", "item": {"id": "minecraft:stone", "count": 0}}))
J(notice(body={"type": "minecraft:item", "item": {"id": "minecraft:air"}}))
J(notice(body={"type": "minecraft:item", "item": "minecraft:air"}))
J(notice(body={"type": "minecraft:item", "item": "minecraft:nonexistent"}))
J(notice(body={"type": "minecraft:item", "item": {"id": "minecraft:diamond_sword", "components": {"minecraft:damage": 3}}}))
J(notice(body={"type": "minecraft:item", "item": {"id": "minecraft:diamond_sword", "components": {"minecraft:bogus": 3}}}))
J(notice(body={"type": "minecraft:item", "item": "minecraft:stone", "description": "d"}))
J(notice(body={"type": "minecraft:item", "item": "minecraft:stone", "description": {"contents": "d", "width": 33}}))
J(notice(body={"type": "minecraft:item", "item": "minecraft:stone", "description": {"width": 33}}))
J(notice(body={"type": "minecraft:item", "item": "minecraft:stone", "show_decorations": False, "show_tooltip": False, "width": 256, "height": 1}))
J(notice(body={"type": "minecraft:item", "item": "minecraft:stone", "width": 257}))
J(notice(body={"type": "minecraft:item", "item": "minecraft:stone", "height": 0}))
J(notice(body={"type": "minecraft:item"}))

C("inputs")
J(notice(inputs=[]))
J(notice(inputs=[{"type": "minecraft:boolean", "key": "k", "label": "L"}]))
J(notice(inputs=[{"type": "minecraft:boolean", "key": "k", "label": "L", "initial": True, "on_true": "yes", "on_false": "no"}]))
J(notice(inputs=[{"type": "minecraft:boolean", "key": "k"}]))
J(notice(inputs=[{"type": "minecraft:boolean", "label": "L"}]))
J(notice(inputs=[{"type": "minecraft:boolean", "key": "bad key", "label": "L"}]))
J(notice(inputs=[{"type": "minecraft:boolean", "key": "", "label": "L"}]))
J(notice(inputs=[{"type": "minecraft:boolean", "key": "ünï_9", "label": "L"}]))
J(notice(inputs=[{"type": "minecraft:boolean", "key": "a-b", "label": "L"}]))
J(notice(inputs=[{"type": "minecraft:boolean", "key": "x\u00b2", "label": "L"}]))
J(notice(inputs=[{"type": "minecraft:boolean", "key": "\ud835\udc00", "label": "L"}]))
J(notice(inputs=[{"type": "minecraft:boolean", "key": "k", "label": "L", "on_true": 5}]))
J(notice(inputs=[{"type": "minecraft:number_range", "key": "n", "label": "L", "start": 0, "end": 10}]))
J(notice(inputs=[{"type": "minecraft:number_range", "key": "n", "label": "L", "start": 0, "end": 10, "initial": 5, "step": 1, "width": 300, "label_format": "x.y"}]))
J(notice(inputs=[{"type": "minecraft:number_range", "key": "n", "label": "L", "start": 10, "end": 0, "initial": 5}]))
J(notice(inputs=[{"type": "minecraft:number_range", "key": "n", "label": "L", "start": 0, "end": 10, "initial": 11}]))
J(notice(inputs=[{"type": "minecraft:number_range", "key": "n", "label": "L", "start": 0, "end": 10, "initial": -0.5}]))
J(notice(inputs=[{"type": "minecraft:number_range", "key": "n", "label": "L", "start": 0.1, "end": 0.7, "initial": 0.7}]))
J(notice(inputs=[{"type": "minecraft:number_range", "key": "n", "label": "L", "start": 0, "end": 10, "step": 0}]))
J(notice(inputs=[{"type": "minecraft:number_range", "key": "n", "label": "L", "start": 0, "end": 10, "step": -1}]))
J(notice(inputs=[{"type": "minecraft:number_range", "key": "n", "label": "L", "start": 0, "end": 10, "step": 0.25}]))
J(notice(inputs=[{"type": "minecraft:number_range", "key": "n", "label": "L", "start": 3, "end": 3, "initial": 3}]))
J(notice(inputs=[{"type": "minecraft:number_range", "key": "n", "label": "L", "start": "1", "end": "2"}]))
J(notice(inputs=[{"type": "minecraft:number_range", "key": "n", "label": "L", "start": 0}]))
J(notice(inputs=[{"type": "minecraft:number_range", "key": "n", "label": "L", "start": 0, "end": 1, "width": 0}]))
J(notice(inputs=[{"type": "minecraft:single_option", "key": "s", "label": "L", "options": ["a", "b"]}]))
J(notice(inputs=[{"type": "minecraft:single_option", "key": "s", "label": "L", "options": [{"id": "a", "display": "A", "initial": True}, "b"]}]))
J(notice(inputs=[{"type": "minecraft:single_option", "key": "s", "label": "L", "options": [{"id": "a", "initial": True}, {"id": "b", "initial": True}]}]))
J(notice(inputs=[{"type": "minecraft:single_option", "key": "s", "label": "L", "options": []}]))
J(notice(inputs=[{"type": "minecraft:single_option", "key": "s", "label": "L"}]))
J(notice(inputs=[{"type": "minecraft:single_option", "key": "s", "label": "L", "options": [{"display": "x"}]}]))
J(notice(inputs=[{"type": "minecraft:single_option", "key": "s", "label": "L", "options": ["a"], "width": 5, "label_visible": False}]))
J(notice(inputs=[{"type": "minecraft:single_option", "key": "s", "label": "L", "options": [5]}]))
J(notice(inputs=[{"type": "minecraft:text", "key": "t", "label": "L"}]))
J(notice(inputs=[{"type": "minecraft:text", "key": "t", "label": "L", "initial": "abc", "max_length": 3, "label_visible": False, "width": 1}]))
J(notice(inputs=[{"type": "minecraft:text", "key": "t", "label": "L", "initial": "abcd", "max_length": 3}]))
J(notice(inputs=[{"type": "minecraft:text", "key": "t", "label": "L", "initial": "\ud83d\ude00", "max_length": 2}]))
J(notice(inputs=[{"type": "minecraft:text", "key": "t", "label": "L", "initial": "\ud83d\ude00", "max_length": 1}]))
J(notice(inputs=[{"type": "minecraft:text", "key": "t", "label": "L", "initial": "x" * 33}]))
J(notice(inputs=[{"type": "minecraft:text", "key": "t", "label": "L", "max_length": 0}]))
J(notice(inputs=[{"type": "minecraft:text", "key": "t", "label": "L", "multiline": {}}]))
J(notice(inputs=[{"type": "minecraft:text", "key": "t", "label": "L", "multiline": {"max_lines": 4, "height": 512}}]))
J(notice(inputs=[{"type": "minecraft:text", "key": "t", "label": "L", "multiline": {"max_lines": 0}}]))
J(notice(inputs=[{"type": "minecraft:text", "key": "t", "label": "L", "multiline": {"height": 513}}]))
J(notice(inputs=[{"type": "minecraft:text", "key": "t", "label": "L", "multiline": {"height": 0}}]))
J(notice(inputs=[{"type": "minecraft:text", "key": "t", "label": "L", "multiline": 5}]))
J(notice(inputs=[{"type": "minecraft:slider", "key": "t", "label": "L"}]))
J(notice(inputs=[{"key": "t", "label": "L"}]))
J(notice(inputs={"type": "minecraft:boolean", "key": "k", "label": "L"}))
J(notice(inputs=[{"type": "minecraft:boolean", "key": "k", "label": "L"}, {"type": "minecraft:text", "key": "k", "label": "M"}]))

C("actions")
for a in [
    {"type": "minecraft:open_url", "url": "https://example.org/a b"},
    {"type": "open_url", "url": "https://example.org"},
    {"type": "minecraft:open_url", "url": "http://example.org"},
    {"type": "minecraft:open_url", "url": "ftp://example.org"},
    {"type": "minecraft:open_url", "url": "example.org"},
    {"type": "minecraft:open_url"},
    {"type": "minecraft:open_file", "path": "x"},
    {"type": "minecraft:run_command", "command": "say hi"},
    {"type": "minecraft:run_command", "command": "say \u00a7 hi"},
    {"type": "minecraft:run_command", "command": "say\nhi"},
    {"type": "minecraft:run_command"},
    {"type": "minecraft:suggest_command", "command": "/say hi"},
    {"type": "minecraft:suggest_command", "command": "\u0000"},
    {"type": "minecraft:change_page", "page": 3},
    {"type": "minecraft:change_page", "page": 0},
    {"type": "minecraft:change_page", "page": 3.5},
    {"type": "minecraft:change_page", "page": "3"},
    {"type": "minecraft:copy_to_clipboard", "value": "v"},
    {"type": "minecraft:copy_to_clipboard", "value": 5},
    {"type": "minecraft:custom", "id": "ns:a"},
    {"type": "minecraft:custom", "id": "ns:a", "payload": {"a": 1, "b": [1, 2], "c": "s", "d": 1.5, "e": True}},
    {"type": "minecraft:custom", "id": "ns:a", "payload": "str"},
    {"type": "minecraft:custom", "id": "ns:a", "payload": 5},
    {"type": "minecraft:custom", "id": "Bad Id"},
    {"type": "minecraft:custom"},
    {"type": "minecraft:show_dialog", "dialog": "minecraft:server_links"},
    {"type": "minecraft:show_dialog", "dialog": "minecraft:nope"},
    {"type": "minecraft:show_dialog", "dialog": "Bad Id"},
    {"type": "minecraft:show_dialog", "dialog": notice()},
    {"type": "minecraft:show_dialog", "dialog": notice(body="x", pause=False, after_action="none")},
    {"type": "minecraft:show_dialog", "dialog": {"type": "minecraft:notice"}},
    {"type": "minecraft:show_dialog", "dialog": 5},
    {"type": "minecraft:show_dialog", "dialog": [notice()]},
    {"type": "minecraft:show_dialog"},
    {"type": "minecraft:show_dialog", "dialog": {"type": "minecraft:notice", **T, "action": btn(action={"type": "minecraft:show_dialog", "dialog": notice()})}},
    {"type": "minecraft:dynamic/run_command", "template": "say $(a) and $(b)"},
    {"type": "minecraft:dynamic/run_command", "template": "say hi"},
    {"type": "minecraft:dynamic/run_command", "template": "say $(a"},
    {"type": "minecraft:dynamic/run_command", "template": "say $(a-b)"},
    {"type": "minecraft:dynamic/run_command", "template": "say $(a) $"},
    {"type": "minecraft:dynamic/run_command", "template": "$$(a)"},
    {"type": "minecraft:dynamic/run_command", "template": "$()"},
    {"type": "minecraft:dynamic/run_command", "template": "$(\u00e9)"},
    {"type": "minecraft:dynamic/run_command", "template": "a\ud83d\ude00$(x)"},
    {"type": "minecraft:dynamic/run_command", "template": 5},
    {"type": "minecraft:dynamic/run_command"},
    {"type": "minecraft:dynamic/custom", "id": "ns:a"},
    {"type": "minecraft:dynamic/custom", "id": "ns:a", "additions": {"a": 1, "b": "x"}},
    {"type": "minecraft:dynamic/custom", "id": "ns:a", "additions": "x"},
    {"type": "minecraft:dynamic/custom", "id": "ns:a", "additions": []},
    {"type": "minecraft:dynamic/custom"},
    {"type": "minecraft:dynamic/other", "id": "ns:a"},
    {"type": "dynamic/custom", "id": "ns:a"},
    {"id": "ns:a"},
    {"type": 1},
    "minecraft:run_command",
]:
    J(notice(action=btn(action=a)))
J(notice(action=btn(action=None)))
J(notice(action=5))
J(notice(action={}))
J(notice(action={"label": "L", "width": 0}))
J(notice(action={"label": "L", "width": 1025}))
J(notice(action={"label": {"text": "x"}, "tooltip": ["a"], "width": 1024}))
J(notice(action={"label": "L", "tooltip": 5}))
J(notice(action={"width": 5}))

C("NBT ops (numeric tag types, booleans as bytes)")
N('{type:"minecraft:notice",title:"T"}')
N('{type:"minecraft:notice",title:"T",pause:0b,after_action:"none"}')
N('{type:"minecraft:notice",title:"T",pause:1b,can_close_with_escape:0b}')
N('{type:"minecraft:notice",title:"T",pause:2}')
N('{type:"minecraft:notice",title:"T",action:{label:"L",width:100b}}')
N('{type:"minecraft:notice",title:"T",action:{label:"L",width:200s}}')
N('{type:"minecraft:notice",title:"T",action:{label:"L",width:200L}}')
N('{type:"minecraft:notice",title:"T",action:{label:"L",width:200.9f}}')
N('{type:"minecraft:notice",title:"T",action:{label:"L",width:200.9d}}')
N('{type:"minecraft:notice",title:"T",action:{label:"L",width:"200"}}')
N('{type:"minecraft:notice",title:"T",action:{label:"L",width:0}}')
N('{type:"minecraft:notice",title:"T",action:{label:"L",action:{type:"minecraft:change_page",page:3b}}}')
N('{type:"minecraft:notice",title:"T",action:{label:"L",action:{type:"minecraft:change_page",page:3.5f}}}')
N('{type:"minecraft:notice",title:"T",action:{label:"L",action:{type:"minecraft:custom",id:"a:b",payload:{x:1b,y:[1,2],z:"s",w:[I;1,2]}}}}')
N('{type:"minecraft:notice",title:"T",action:{label:"L",action:{type:"minecraft:dynamic/custom",id:"a:b",additions:{x:1b,y:2s,z:3L,w:4.5f,v:5.5d,u:[B;1,2],t:"s"}}}}')
N('{type:"minecraft:notice",title:"T",action:{label:"L",action:{type:"minecraft:dynamic/custom",id:"a:b",additions:3}}}')
N('{type:"minecraft:notice",title:"T",action:{label:"L",action:{type:"minecraft:show_dialog",dialog:"minecraft:server_links"}}}')
N('{type:"minecraft:notice",title:"T",action:{label:"L",action:{type:"minecraft:show_dialog",dialog:{type:"minecraft:notice",title:"x"}}}}')
N('{type:"minecraft:notice",title:"T",action:{label:"L",action:{type:"minecraft:show_dialog",dialog:5}}}')
N('{type:"minecraft:number_range",title:"T"}')
N('{type:"minecraft:notice",title:"T",inputs:[{type:"minecraft:number_range",key:"n",label:"L",start:0.0f,end:10.0f,initial:5.0f,step:0.5f}]}')
N('{type:"minecraft:notice",title:"T",inputs:[{type:"minecraft:number_range",key:"n",label:"L",start:0,end:10,initial:5,step:1}]}')
N('{type:"minecraft:notice",title:"T",inputs:[{type:"minecraft:number_range",key:"n",label:"L",start:0.0d,end:10.0d,initial:11.0d}]}')
N('{type:"minecraft:notice",title:"T",inputs:[{type:"minecraft:number_range",key:"n",label:"L",start:0.0f,end:10.0f,initial:11.0f}]}')
N('{type:"minecraft:notice",title:"T",inputs:[{type:"minecraft:number_range",key:"n",label:"L",start:0.0f,end:10.0f,initial:NaNf}]}')
N('{type:"minecraft:notice",title:"T",inputs:[{type:"minecraft:number_range",key:"n",label:"L",start:Infinityf,end:10.0f}]}')
N('{type:"minecraft:notice",title:"T",inputs:[{type:"minecraft:number_range",key:"n",label:"L",start:-0.0f,end:0.0f,initial:-0.0f}]}')
N('{type:"minecraft:notice",title:"T",inputs:[{type:"minecraft:boolean",key:"k",label:"L",initial:1b},{type:"minecraft:text",key:"t",label:"L",max_length:5b,initial:"abc"}]}')
N('{type:"minecraft:notice",title:"T",inputs:[{type:"minecraft:single_option",key:"s",label:"L",options:["a",{id:"b",initial:1b,display:"B"}]}]}')
N('{type:"minecraft:notice",title:"T",body:["a",{type:"minecraft:plain_message",contents:"b",width:5b}]}')
N('{type:"minecraft:notice",title:"T",body:{type:"minecraft:item",item:{id:"minecraft:stone",count:2b}}}')
N('{type:"minecraft:notice",title:"T",body:{type:"minecraft:item",item:{id:"minecraft:diamond_sword",components:{"minecraft:damage":3b}}}}')
N('{type:"minecraft:notice",title:"T",body:{type:"minecraft:item",item:"minecraft:stone",show_tooltip:0b,width:16b,height:20s}}')
N('{type:"minecraft:dialog_list",title:"T",dialogs:"#minecraft:quick_actions"}')
N('{type:"minecraft:dialog_list",title:"T",dialogs:["minecraft:server_links"]}')
N('{type:"minecraft:dialog_list",title:"T",dialogs:"minecraft:server_links",columns:3b}')
N('{type:"minecraft:confirmation",title:{text:"T",bold:1b},yes:{label:"Y"},no:{label:"N",tooltip:"t"}}')
N('"minecraft:server_links"')
N('"minecraft:nope"')
N('{}')
N('5')
N('[]')

C("stream codecs (STREAM_CODEC holder: 0 + NBT, or id+1; CONTEXT_FREE: NBT only)")
lines.append('S{type:"minecraft:notice",title:"T"}')
lines.append('S{type:"minecraft:notice"}')
lines.append('S{}')
lines.append('S"x"')
lines.append('X')
lines.append('X01')
lines.append('X02')
lines.append('X03')
lines.append('X04')
lines.append('Xff01')
lines.append('X00')
lines.append('X0000')
lines.append('X000a')
lines.append('X0a00')
lines.append('C{type:"minecraft:notice",title:"T"}')
lines.append('C{type:"minecraft:notice"}')
lines.append('C{type:"minecraft:confirmation",title:"T",yes:{label:"Y"},no:{label:"N"}}')
lines.append('C"minecraft:server_links"')
lines.append('S{type:"minecraft:dialog_list",title:"T",dialogs:"#minecraft:quick_actions"}')
lines.append('S{type:"minecraft:dialog_list",title:"T",dialogs:["minecraft:nope"]}')

open(os.path.join(os.path.dirname(__file__), "corpus.txt"), "w").write(
    "# Generated by gen_corpus.py: regenerate with it, then gen.sh.\n" + "\n".join(lines) + "\n")
print(len(lines), "lines")
