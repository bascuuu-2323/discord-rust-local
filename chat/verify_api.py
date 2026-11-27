"""Exercise the documented text-message routes against an isolated local database."""
import json
from pathlib import Path
import subprocess
import os
import tempfile
import sys
import zipfile
from contextlib import contextmanager
from urllib.error import HTTPError
from urllib.request import Request, urlopen

ROOT = Path(__file__).resolve().parents[1]
EXE = ROOT / "target" / "debug" / ("discord-server.exe" if os.name == "nt" else "discord-server")

# Optional contract check against the exact Discord OpenAPI archive under review.
schemas = None
if len(sys.argv) > 1:
    with zipfile.ZipFile(sys.argv[1]) as archive:
        spec = next(n for n in archive.namelist() if n.endswith("/specs/openapi.json"))
        schemas = json.loads(archive.read(spec))["components"]["schemas"]

def verify_callback(callback, component):
    if schemas is None:
        return
    reply_schema = schemas["CreateMessageInteractionCallbackRequest"]
    text_schema = schemas["TextDisplayComponentForMessageRequest"]
    assert callback["type"] in reply_schema["properties"]["type"]["enum"]
    assert set(text_schema["required"]) <= component.keys()
    assert component["type"] in text_schema["properties"]["type"]["enum"]
    text = text_schema["properties"]["content"]
    assert text["minLength"] <= len(component["content"]) <= text["maxLength"]

@contextmanager
def instance(db):
    proc = subprocess.Popen([EXE], stdout=subprocess.PIPE, text=True, env={
        **os.environ, "DISCORD_LOCAL_PORT":"0", "DISCORD_LOCAL_DB":str(db)})
    try:
        url = proc.stdout.readline().strip()
        assert url.startswith("http://127.0.0.1:")
        def call(method, path, token=None, body=None, status=200):
            headers = {"Content-Type":"application/json"}
            if token: headers["Authorization"] = "Bearer " + token
            req = Request(url+path, None if body is None else json.dumps(body).encode(), headers, method=method)
            try: response = urlopen(req, timeout=5)
            except HTTPError as e: response = e
            with response:
                data = response.read()
                assert response.status == status, (method,path,response.status,data)
                return json.loads(data) if data else None
        yield call
    finally:
        proc.terminate()
        proc.wait(timeout=5)

with tempfile.TemporaryDirectory() as tmp:
    db = Path(tmp)/"test.sqlite"
    with instance(db) as call:
        login = lambda name: call("POST","/local/sessions",body={"username":name})["token"]
        a,b = login("Ana"),login("Luis")
        channels = call("GET","/api/v10/guilds/4194304/channels",a)
        path,other = [f"/api/v10/channels/{c['id']}/messages" for c in channels]
        channel_route = "/api/v10/guilds/4194304/channels"
        call("POST", channel_route, body={"name":"Proyecto"}, status=401)
        call("POST", channel_route, a, {"name":"  "}, 400)
        call("POST", channel_route, a, {"name":"Voz","type":2}, 400)
        created = call("POST", channel_route, a, {"name":"Mi Proyecto","type":0})
        assert created["name"] == "mi-proyecto"
        assert created in call("GET", channel_route, b)
        created_path = f"/api/v10/channels/{created['id']}/messages"
        posted = call("POST", created_path, b, {"content":"Canal nuevo"})
        assert call("GET", created_path, a) == [posted]
        preview = f"/local/channels/{created['id']}/interactions"
        command = {"type":2, "data":{"name":"test"}, "locale":"es-ES"}
        call("POST", preview, body=command, status=401)
        callback = call("POST", preview, b, command)
        assert callback["type"] == 4 and callback["data"]["flags"] == 32768
        component, = callback["data"]["components"]
        assert component["type"] == 10 and component["content"].startswith("hola mundo ")
        verify_callback(callback, component)
        call("POST", preview, b, {"type":2,"data":{"name":"inexistente"}}, 400)
        call("POST", "/local/channels/0/interactions", b, command, 404)
        call("POST", "/interactions", body={"type":1}, status=401)
        args = {"cmd":"GET_CHANNEL", "nonce":"preview-1", "args":{"channel_id":created["id"]}}
        call("POST", "/local/activities/rpc", body=args, status=401)
        reply = call("POST", "/local/activities/rpc", a, args)
        assert (reply["cmd"], reply["evt"], reply["nonce"]) == ("GET_CHANNEL", None, "preview-1")
        assert reply["data"]["name"] == "mi-proyecto"
        recent, = reply["data"]["messages"]
        assert recent["id"] == posted["id"] and recent["author"]["bot"] is False
        assert call("GET", created_path, a) == [posted]
        call("GET",path,status=401)
        call("POST",path,a,{"content":" "},400)
        call("POST",path,a,{"content":"x"*2001},400)
        msgs = [call("POST",path,a if i%2==0 else b,{"content":f"Mensaje {i}"}) for i in range(7)]
        ids = [m["id"] for m in msgs]
        assert all(isinstance(i,str) and i.isdecimal() for i in ids)
        def history(params=""): return [m["id"] for m in call("GET",path+params,b)]
        assert history()==ids[::-1]
        assert history("?before="+ids[3]+"&limit=2")==[ids[2],ids[1]]
        assert history("?after="+ids[2]+"&limit=2")==[ids[4],ids[3]]
        assert history("?around="+ids[3]+"&limit=3")==[ids[4],ids[3],ids[2]]
        for query in ["?limit=0","?limit=101","?after=1&around=2"]:
            call("GET",path+query,a,status=400)
        assert call("GET",other,b)==[]
        item=path+"/"+ids[0]
        call("GET",other+"/"+ids[0],b,status=404)
        call("PATCH",item,b,{"content":"otro"},403)
        call("DELETE",item,b,status=403)
        edited=call("PATCH",item,a,{"content":"corregido"})
        assert edited["edited_timestamp"] and edited["content"]=="corregido"
    with instance(db) as call:
        assert created in call("GET", channel_route, b)
        assert call("GET", created_path, b) == [posted]
        assert call("GET",item,b)==edited
        call("DELETE",item,a,status=204)
        call("GET",item,b,status=404)
print("OK: sessions, channel creation, message lifecycle, ownership, pagination, isolation and restart")

if schemas is not None:
    print("OK: example callback matches the selected Discord OpenAPI fields")
