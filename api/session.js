const API='https://discord.com/api/v10';
function json(res,status,data){res.statusCode=status;res.setHeader('Content-Type','application/json; charset=utf-8');res.end(JSON.stringify(data));}
export default async function handler(req,res){
  if(req.method==='GET') return json(res,200,{ok:true});
  if(req.method!=='POST') return json(res,405,{message:'Method not allowed'});
  try{
    let raw=''; for await(const c of req) raw+=c;
    const body=raw?JSON.parse(raw):{};
    const token=String(body.token||'').trim();
    if(!token) return json(res,400,{message:'Bot token required'});
    const h={Authorization:`Bot ${token}`};
    const meR=await fetch(`${API}/users/@me`,{headers:h});
    const meText=await meR.text();
    if(!meR.ok) return json(res,meR.status,{message:'Invalid bot token',discord:tryJson(meText)});
    const me=JSON.parse(meText);
    const gsR=await fetch(`${API}/users/@me/guilds`,{headers:h});
    const gsText=await gsR.text();
    if(!gsR.ok) return json(res,gsR.status,{message:'Could not load servers',discord:tryJson(gsText)});
    const guilds=JSON.parse(gsText);
    res.setHeader('Set-Cookie',`mc_token=${encodeURIComponent(token)}; Path=/; HttpOnly; Secure; SameSite=Lax; Max-Age=86400`);
    return json(res,200,{me,guilds});
  }catch(e){return json(res,500,{message:e?.message||'Session error'})}
}
function tryJson(x){try{return JSON.parse(x)}catch{return x}}
