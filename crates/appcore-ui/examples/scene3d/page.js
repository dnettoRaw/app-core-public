import {flightInitial,moveFlight} from './city.mjs';
import {SCENES,createShowcase} from './showcase.mjs';
import {multiply,translate,rotateX,rotateY,perspective,initial,command,createLorenz,advanceLorenz,TRAILS,TRAIL_LENGTH,FIXED_STEP} from './model.mjs';
const canvas=document.querySelector('#scene'),status=document.querySelector('#status'),state=initial();let simulation=createLorenz(),demo='city',geometry=createShowcase('city'),elapsed=0,camera=flightInitial();const keys=new Set();
let gl=null,program=null,buffer=null,projection=null,tint=null,size=null,previous=0,accumulator=0,request=0,lost=false,drag=null,hudTime=0,clock=null,wave=null,round=null,cityMode=null,normalBuffer=null,normalAttribute=null;
const colors=[[.95,.78,.35],[.24,.79,.79],[.50,.64,.98],[.92,.41,.58],[.58,.85,.50],[.76,.50,.92]];
function shader(type,source){const item=gl.createShader(type);if(!item)throw Error('Shader unavailable');gl.shaderSource(item,source);gl.compileShader(item);if(!gl.getShaderParameter(item,gl.COMPILE_STATUS)){const detail=gl.getShaderInfoLog(item)||'No driver diagnostic';gl.deleteShader(item);throw Error('Shader compilation failed: '+detail.slice(0,1024));}return item;}
function initialize(){
 gl=canvas.getContext('webgl',{alpha:false,antialias:true});if(!gl)throw Error('WebGL unavailable on this system');
 const vs=shader(gl.VERTEX_SHADER,'attribute vec3 p; attribute vec3 normal; uniform mediump float cityMode; varying mediump float fogDepth; uniform mat4 m; uniform float pointSize; uniform float clock; uniform float wave; varying mediump float shade; void main(){vec3 v=p;v.y+=wave*(sin(v.x*1.8+clock*2.0)*0.35+cos(v.z*1.5+clock*1.4)*0.25);gl_Position=m*vec4(v,1.0);fogDepth=gl_Position.w;gl_PointSize=pointSize;shade=0.45+0.5*clamp((v.y+1.0)/2.0,0.0,1.0);if(cityMode>0.5)shade=0.25+0.75*max(dot(normal,normalize(vec3(-0.5,0.8,0.4))),0.0);}');
 const fs=shader(gl.FRAGMENT_SHADER,'precision mediump float; uniform vec3 color; uniform float roundPoints; uniform mediump float cityMode; varying mediump float shade; varying mediump float fogDepth; void main(){if(roundPoints>0.5){vec2 q=gl_PointCoord-vec2(0.5);if(dot(q,q)>0.25)discard;}vec3 lit=color*shade;lit=mix(lit,vec3(0.045,0.075,0.11),cityMode*(1.0-exp(-max(fogDepth,0.0)*0.035)));gl_FragColor=vec4(lit,1.0);}');
 program=gl.createProgram();gl.attachShader(program,vs);gl.attachShader(program,fs);gl.linkProgram(program);gl.deleteShader(vs);gl.deleteShader(fs);if(!gl.getProgramParameter(program,gl.LINK_STATUS)){const detail=gl.getProgramInfoLog(program)||'No driver diagnostic';gl.deleteProgram(program);program=null;throw Error('Program linking failed: '+detail.slice(0,1024));}
 gl.useProgram(program);buffer=gl.createBuffer();gl.bindBuffer(gl.ARRAY_BUFFER,buffer);gl.bufferData(gl.ARRAY_BUFFER,geometry?geometry.points:simulation.points,gl.DYNAMIC_DRAW);
 const position=gl.getAttribLocation(program,'p');gl.enableVertexAttribArray(position);gl.vertexAttribPointer(position,3,gl.FLOAT,false,12,0);
 projection=gl.getUniformLocation(program,'m');tint=gl.getUniformLocation(program,'color');size=gl.getUniformLocation(program,'pointSize');gl.enable(gl.DEPTH_TEST);gl.enable(gl.BLEND);gl.blendFunc(gl.SRC_ALPHA,gl.ONE_MINUS_SRC_ALPHA);
 clock=gl.getUniformLocation(program,'clock');wave=gl.getUniformLocation(program,'wave');round=gl.getUniformLocation(program,'roundPoints');
 cityMode=gl.getUniformLocation(program,'cityMode');normalAttribute=gl.getAttribLocation(program,'normal');normalBuffer=gl.createBuffer();bindGeometry();
 lost=false;previous=0;accumulator=0;update();request=requestAnimationFrame(frame);
}
function bindGeometry(){
 gl.bindBuffer(gl.ARRAY_BUFFER,buffer);gl.bufferData(gl.ARRAY_BUFFER,geometry?geometry.points:simulation.points,gl.DYNAMIC_DRAW);
 if(geometry?.normals){gl.bindBuffer(gl.ARRAY_BUFFER,normalBuffer);gl.bufferData(gl.ARRAY_BUFFER,geometry.normals,gl.STATIC_DRAW);gl.enableVertexAttribArray(normalAttribute);gl.vertexAttribPointer(normalAttribute,3,gl.FLOAT,false,12,0);}
 else{gl.disableVertexAttribArray(normalAttribute);gl.vertexAttrib3f(normalAttribute,0,1,0);}
 gl.bindBuffer(gl.ARRAY_BUFFER,buffer);
}
function upload(start,steps){
 if(!steps)return;const first=Math.min(steps,TRAIL_LENGTH-start),second=steps-first;
 gl.bindBuffer(gl.ARRAY_BUFFER,buffer);
 for(let i=0;i<TRAILS;i++){
  const offset=(i*TRAIL_LENGTH+start)*3;gl.bufferSubData(gl.ARRAY_BUFFER,offset*4,simulation.points.subarray(offset,offset+first*3));
  if(second){const offset=i*TRAIL_LENGTH*3;gl.bufferSubData(gl.ARRAY_BUFFER,offset*4,simulation.points.subarray(offset,offset+second*3));}
 }
}
function advance(steps){if(demo!=='lorenz'){elapsed+=steps*FIXED_STEP;return;}const start=simulation.cursor;advanceLorenz(simulation,steps);upload(start,steps);}
function update(){
 document.querySelector('#motion').textContent=state.motion?'Pause motion':'Resume motion';document.querySelector('#explode').textContent=state.explode?'Show all trails':'Isolate trail';
 document.querySelector('#demo').value=demo;document.querySelector('#near').disabled=lost||demo==='city';document.querySelector('#far').disabled=lost||demo==='city';
 document.querySelector('#title').textContent=SCENES[demo].name;document.querySelector('#description').textContent=SCENES[demo].description;document.querySelector('#budget').textContent=geometry?`${geometry.points.length/3} VERTICES / ${geometry.triangles?geometry.points.length/9+' TRIANGLES':'POINTS'}`:'6 TRAILS / 24,576 POINT LIMIT';
 document.querySelector('#select').disabled=lost||demo!=='lorenz';document.querySelector('#explode').disabled=lost||demo!=='lorenz';
 document.querySelector('#selection').textContent=`Trail ${state.selected+1} of ${TRAILS} · seed Δx = ${(state.selected*.001).toFixed(3)} · command revision ${state.revision}`;
 if(demo!=='lorenz'){document.querySelector('#selection').textContent=`${SCENES[demo].name} · command revision ${state.revision}`;status.textContent=`${state.motion?'Animating':'Paused'} · t = ${elapsed.toFixed(2)} · WebGL`;return;}
 status.textContent=`${state.motion?'Simulating':'Paused'} · ${simulation.count*TRAILS} retained points · t = ${(simulation.steps*FIXED_STEP).toFixed(2)}`;
}
for(const id of ['motion','select','explode','reset','near','far','step'])document.querySelector('#'+id).onclick=()=>{
 command(state,id);
 if(id==='reset'){camera=flightInitial();keys.clear();elapsed=0;simulation=createLorenz();accumulator=0;bindGeometry();}
 if(id==='step')advance(8);update();
};
document.querySelector('#demo').onchange=e=>{keys.clear();camera=flightInitial();demo=e.target.value;geometry=demo==='lorenz'?null:createShowcase(demo);elapsed=0;accumulator=0;previous=0;command(state,'reset');simulation=createLorenz();bindGeometry();update();};
canvas.addEventListener('wheel',e=>{if(demo==='city')return;e.preventDefault();command(state,e.deltaY<0?'near':'far');update();},{passive:false});
canvas.addEventListener('pointerdown',e=>{canvas.focus();drag=[e.clientX,e.clientY];canvas.setPointerCapture(e.pointerId);});canvas.addEventListener('pointermove',e=>{if(!drag)return;if(demo==='city'){camera.yaw-=(e.clientX-drag[0])*.005;camera.pitch=Math.max(-1.45,Math.min(1.45,camera.pitch-(e.clientY-drag[1])*.005));drag=[e.clientX,e.clientY];return;}state.yaw+=(e.clientX-drag[0])*.008;state.pitch=Math.max(-1.3,Math.min(1.3,state.pitch+(e.clientY-drag[1])*.008));state.revision++;drag=[e.clientX,e.clientY];});for(const event of ['pointerup','pointercancel','lostpointercapture'])canvas.addEventListener(event,()=>{drag=null;});
canvas.addEventListener('keydown',e=>{if(demo!=='city')return;if(['KeyW','KeyA','KeyS','KeyD','KeyQ','KeyE','ShiftLeft','ShiftRight'].includes(e.code)){e.preventDefault();keys.add(e.code);}if(e.code==='KeyR'){camera=flightInitial();keys.clear();}if(e.code==='Escape'){keys.clear();canvas.blur();}});
canvas.addEventListener('keyup',e=>keys.delete(e.code));canvas.addEventListener('blur',()=>keys.clear());window.addEventListener('blur',()=>keys.clear());
document.addEventListener('visibilitychange',()=>{keys.clear();previous=0;accumulator=0;});
function frame(now){
 if(lost)return;const dt=previous?Math.min(.04,(now-previous)/1000):0;previous=now;
 if(demo==='city'&&!document.hidden)moveFlight(camera,keys,dt);
 if(state.motion&&!document.hidden&&demo==='lorenz'){accumulator=Math.min(.04,accumulator+dt);const steps=Math.min(8,Math.floor((accumulator+1e-9)/FIXED_STEP));if(steps){advance(steps);accumulator-=steps*FIXED_STEP;}}
 if(state.motion&&!document.hidden&&demo!=='lorenz')elapsed+=dt;
 const dpr=Math.min(2,window.devicePixelRatio||1),width=Math.max(1,Math.min(1920,Math.round(canvas.clientWidth*dpr))),height=Math.round(width*420/960);
 if(canvas.width!==width||canvas.height!==height){canvas.width=width;canvas.height=height;}gl.viewport(0,0,width,height);gl.clearColor(.045,.075,.11,1);gl.clear(gl.COLOR_BUFFER_BIT|gl.DEPTH_BUFFER_BIT);
 const view=demo==='city'?multiply(multiply(rotateX(-camera.pitch),rotateY(-camera.yaw)),translate(...camera.position.map(v=>-v))):multiply(multiply(translate(0,0,-state.distance),rotateX(state.pitch)),rotateY(state.yaw+(demo==='galaxy'?elapsed*.12:0)));gl.uniformMatrix4fv(projection,false,multiply(perspective(width/height),view));
 gl.uniform1f(cityMode,demo==='city'?1:0);gl.uniform1f(clock,elapsed);gl.uniform1f(wave,demo==='waves'?1:0);gl.uniform1f(round,geometry?.triangles?0:1);
 if(geometry){gl.uniform3fv(tint,demo==='city'?[.53,.8,.95]:demo==='terrain'?[.45,.85,.58]:demo==='waves'?[.25,.7,1]:[.95,.78,.55]);gl.uniform1f(size,2*dpr);gl.drawArrays(geometry.triangles?gl.TRIANGLES:gl.POINTS,0,geometry.points.length/3);}
 else for(let i=0;i<TRAILS;i++){
  if(state.explode&&i!==state.selected)continue;
  gl.uniform3fv(tint,colors[i]);gl.uniform1f(size,(i===state.selected?2.5:1.6)*dpr);gl.drawArrays(gl.POINTS,i*TRAIL_LENGTH,simulation.count);
  gl.uniform3fv(tint,[1,1,1]);gl.uniform1f(size,7*dpr);gl.drawArrays(gl.POINTS,i*TRAIL_LENGTH+(simulation.cursor+TRAIL_LENGTH-1)%TRAIL_LENGTH,1);
 }
 if(now-hudTime>150){update();hudTime=now;}request=requestAnimationFrame(frame);
}
canvas.addEventListener('webglcontextlost',e=>{e.preventDefault();keys.clear();lost=true;cancelAnimationFrame(request);status.textContent='Context lost — waiting for system recovery';document.querySelectorAll('button,select').forEach(b=>b.disabled=true);});
canvas.addEventListener('webglcontextrestored',()=>{try{document.querySelectorAll('button,select').forEach(b=>b.disabled=false);initialize();}catch(error){fail(error);}});
function fail(error){status.textContent=error.message;document.querySelectorAll('button,select').forEach(b=>b.disabled=true);}
window.addEventListener('pagehide',()=>{lost=true;cancelAnimationFrame(request);if(gl&&!gl.isContextLost()){gl.deleteBuffer(buffer);gl.deleteBuffer(normalBuffer);gl.deleteProgram(program);}});
try{initialize();}catch(error){fail(error);}
